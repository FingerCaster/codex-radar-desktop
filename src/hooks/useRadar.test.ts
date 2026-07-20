import { describe, expect, it } from "vitest";

import { sampleSnapshot } from "../test/fixtures";
import type { RadarSnapshot } from "../types/radar";
import {
  createInitialRadarState,
  radarReducer,
  type RadarViewState,
} from "./useRadar";

const failure = {
  source: "main" as const,
  kind: "network",
  message: "offline",
  occurredAt: "2026-07-19T14:05:00Z",
};

const distributedSnapshot: RadarSnapshot = {
  ...sampleSnapshot,
  source: "distributed",
  updatedAt: "2026-07-20T02:49:31+08:00",
  sourceUrl: "https://api.codexradar.com/api/v1/table",
};

describe("radarReducer", () => {
  it("treats a matching disk cache as stale until the backend confirms it", () => {
    expect(createInitialRadarState("main", sampleSnapshot).status).toBe(
      "stale",
    );
    expect(
      createInitialRadarState("distributed", sampleSnapshot).snapshot,
    ).toBeNull();
  });

  it("keeps the last snapshot visible during same-source refresh and failure", () => {
    const ready: RadarViewState = {
      source: "main",
      activationEpoch: 0,
      snapshot: sampleSnapshot,
      status: "ready",
      error: null,
      notificationsEnabled: true,
    };
    const refreshing = radarReducer(ready, {
      type: "refresh-started",
      source: "main",
    });
    expect(refreshing.snapshot).toBe(sampleSnapshot);
    expect(refreshing.status).toBe("refreshing");

    const stale = radarReducer(refreshing, {
      type: "refresh-failed",
      source: "main",
      failure,
    });
    expect(stale.snapshot).toBe(sampleSnapshot);
    expect(stale.status).toBe("stale");
  });

  it("does not invent a model when the first request fails", () => {
    const unavailable = radarReducer(createInitialRadarState("main"), {
      type: "refresh-failed",
      source: "main",
      failure,
    });
    expect(unavailable.snapshot).toBeNull();
    expect(unavailable.status).toBe("unavailable");
  });

  it("replaces or clears the visible snapshot when the selected source changes", () => {
    const mainReady: RadarViewState = {
      source: "main",
      activationEpoch: 0,
      snapshot: sampleSnapshot,
      status: "ready",
      error: null,
      notificationsEnabled: true,
    };
    const distributedCached = radarReducer(mainReady, {
      type: "source-selected",
      source: "distributed",
      activationEpoch: 1,
      cached: distributedSnapshot,
    });
    expect(distributedCached.source).toBe("distributed");
    expect(distributedCached.snapshot).toBe(distributedSnapshot);
    expect(distributedCached.status).toBe("stale");
    expect(distributedCached.notificationsEnabled).toBe(true);

    const cleared = radarReducer(mainReady, {
      type: "source-selected",
      source: "distributed",
      activationEpoch: 1,
      cached: null,
    });
    expect(cleared.snapshot).toBeNull();
    expect(cleared.status).toBe("booting");
  });

  it("ignores delayed successes and failures from a deselected source", () => {
    const distributedReady: RadarViewState = {
      source: "distributed",
      activationEpoch: 1,
      snapshot: distributedSnapshot,
      status: "ready",
      error: null,
      notificationsEnabled: true,
    };

    expect(
      radarReducer(distributedReady, {
        type: "snapshot-received",
        source: "main",
        snapshot: sampleSnapshot,
      }),
    ).toBe(distributedReady);
    expect(
      radarReducer(distributedReady, {
        type: "refresh-failed",
        source: "main",
        failure,
      }),
    ).toBe(distributedReady);
  });

  it("ignores a superseded refresh from an earlier activation of the same source", () => {
    const mainReady: RadarViewState = {
      source: "main",
      activationEpoch: 0,
      snapshot: sampleSnapshot,
      status: "ready",
      error: null,
      notificationsEnabled: true,
    };

    expect(
      radarReducer(mainReady, {
        type: "refresh-failed",
        source: "main",
        failure: {
          ...failure,
          kind: "superseded",
        },
      }),
    ).toBe(mainReady);
  });

  it("rejects an action whose declared source disagrees with its payload", () => {
    const mainReady = createInitialRadarState("main", sampleSnapshot);
    expect(
      radarReducer(mainReady, {
        type: "snapshot-received",
        source: "main",
        snapshot: distributedSnapshot,
      }),
    ).toBe(mainReady);
    expect(
      radarReducer(mainReady, {
        type: "refresh-failed",
        source: "main",
        failure: { ...failure, source: "distributed" },
      }),
    ).toBe(mainReady);
  });

  it("does not let an older same-source result replace last-known-good data", () => {
    const current = {
      ...sampleSnapshot,
      updatedAt: "2026-07-19T22:00:00+08:00",
    };
    const older = {
      ...sampleSnapshot,
      updatedAt: "2026-07-19T20:00:00+08:00",
    };
    const result = radarReducer(
      {
        source: "main",
        activationEpoch: 0,
        snapshot: current,
        status: "refreshing",
        error: null,
        notificationsEnabled: true,
      },
      { type: "snapshot-received", source: "main", snapshot: older },
    );

    expect(result.snapshot).toBe(current);
    expect(result.status).toBe("stale");
    expect(result.error?.kind).toBe("stale_payload");
    expect(result.error?.source).toBe("main");
  });

  it("reinitializes a reactivated source even when its enum matches again", () => {
    const mainReady: RadarViewState = {
      source: "main",
      activationEpoch: 0,
      snapshot: sampleSnapshot,
      status: "ready",
      error: null,
      notificationsEnabled: true,
    };
    const reactivatedCache = {
      ...sampleSnapshot,
      checkedAt: "2026-07-20T04:00:00Z",
    };

    const result = radarReducer(mainReady, {
      type: "source-selected",
      source: "main",
      activationEpoch: 2,
      cached: reactivatedCache,
    });

    expect(result.activationEpoch).toBe(2);
    expect(result.snapshot).toBe(reactivatedCache);
    expect(result.status).toBe("stale");
  });
});
