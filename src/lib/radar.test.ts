import { openUrl } from "@tauri-apps/plugin-opener";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { sampleSnapshot } from "../test/fixtures";
import type { RadarSnapshot } from "../types/radar";
import {
  getSourceSiteUrl,
  isRadarSnapshot,
  loadCachedSnapshot,
  normalizeRefreshFailure,
  openSourceSite,
  saveCachedSnapshot,
} from "./radar";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(async () => undefined),
}));

const V1_CACHE_KEY = "model-radar:last-snapshot:v1";
const MAIN_CACHE_KEY = "model-radar:last-snapshot:v2:main";
const DISTRIBUTED_CACHE_KEY =
  "model-radar:last-snapshot:v2:distributed";

const distributedSnapshot: RadarSnapshot = {
  ...sampleSnapshot,
  source: "distributed",
  updatedAt: "2026-07-20T02:49:31+08:00",
  attribution: {
    text: "数据来自分布式雷达",
    url: "https://deng.codexradar.com",
  },
  sourceUrl: "https://api.codexradar.com/api/v1/table",
};

describe("snapshot cache boundary", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.clearAllMocks();
  });

  it("round-trips snapshots only through their source-specific v2 cache", () => {
    saveCachedSnapshot(sampleSnapshot);
    saveCachedSnapshot(distributedSnapshot);

    expect(loadCachedSnapshot("main")).toEqual(sampleSnapshot);
    expect(loadCachedSnapshot("distributed")).toEqual(distributedSnapshot);
    expect(window.localStorage.getItem(MAIN_CACHE_KEY)).not.toBeNull();
    expect(window.localStorage.getItem(DISTRIBUTED_CACHE_KEY)).not.toBeNull();
  });

  it("rejects missing, invalid, and cross-source snapshot discriminators", () => {
    expect(isRadarSnapshot({ ...sampleSnapshot, source: undefined })).toBe(
      false,
    );
    expect(isRadarSnapshot({ ...sampleSnapshot, source: "other" })).toBe(
      false,
    );

    window.localStorage.setItem(
      MAIN_CACHE_KEY,
      JSON.stringify(distributedSnapshot),
    );
    expect(loadCachedSnapshot("main")).toBeNull();
  });

  it("migrates a valid fixed-endpoint v1 cache only into main", () => {
    const legacy: Record<string, unknown> = { ...sampleSnapshot };
    delete legacy.source;
    window.localStorage.setItem(V1_CACHE_KEY, JSON.stringify(legacy));

    expect(loadCachedSnapshot("distributed")).toBeNull();
    expect(window.localStorage.getItem(V1_CACHE_KEY)).not.toBeNull();
    expect(loadCachedSnapshot("main")).toEqual(sampleSnapshot);
    expect(window.localStorage.getItem(V1_CACHE_KEY)).toBeNull();
    expect(JSON.parse(window.localStorage.getItem(MAIN_CACHE_KEY) ?? "null"))
      .toEqual(sampleSnapshot);
  });

  it("does not migrate a v1 cache that points outside the fixed primary endpoint", () => {
    const legacy: Record<string, unknown> = {
      ...sampleSnapshot,
      sourceUrl: "https://deng.codexradar.com/current.json",
    };
    delete legacy.source;
    window.localStorage.setItem(V1_CACHE_KEY, JSON.stringify(legacy));

    expect(loadCachedSnapshot("main")).toBeNull();
    expect(window.localStorage.getItem(MAIN_CACHE_KEY)).toBeNull();
  });

  it("rejects malformed and non-finite cached scores", () => {
    expect(isRadarSnapshot({ rankings: [] })).toBe(false);
    expect(
      isRadarSnapshot({
        ...sampleSnapshot,
        rankings: [{ ...sampleSnapshot.rankings[0], score: Number.NaN }],
      }),
    ).toBe(false);
    expect(
      isRadarSnapshot({
        ...sampleSnapshot,
        rankings: [
          {
            ...sampleSnapshot.rankings[0],
            averageTaskTimeHuman: {},
          },
        ],
      }),
    ).toBe(false);
    expect(
      isRadarSnapshot({ ...sampleSnapshot, updatedAt: "not-a-date" }),
    ).toBe(false);
  });

  it("ignores invalid JSON in source-specific local storage", () => {
    window.localStorage.setItem(MAIN_CACHE_KEY, "{bad json");
    expect(loadCachedSnapshot("main")).toBeNull();
  });
});

describe("source boundary", () => {
  beforeEach(() => vi.clearAllMocks());

  it("opens only the fixed site mapped from the source enum", async () => {
    expect(getSourceSiteUrl("main")).toBe("https://codexradar.com");
    expect(getSourceSiteUrl("distributed")).toBe(
      "https://deng.codexradar.com",
    );

    await openSourceSite("main");
    await openSourceSite("distributed");

    expect(openUrl).toHaveBeenNthCalledWith(1, "https://codexradar.com");
    expect(openUrl).toHaveBeenNthCalledWith(
      2,
      "https://deng.codexradar.com",
    );
  });

  it("preserves a valid failure source and applies a known fallback otherwise", () => {
    expect(
      normalizeRefreshFailure(
        {
          source: "distributed",
          kind: "network",
          message: "offline",
          occurredAt: "2026-07-20T03:00:00Z",
        },
        "main",
      ).source,
    ).toBe("distributed");
    expect(normalizeRefreshFailure(new Error("offline"), "main")).toMatchObject(
      {
        source: "main",
        kind: "unknown",
        message: "offline",
      },
    );
  });
});
