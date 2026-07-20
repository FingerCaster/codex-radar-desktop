import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
  isTauri: () => false,
}));

import { sampleSnapshot } from "../test/fixtures";
import { DEFAULT_DESKTOP_PREFERENCES } from "../types/desktop";
import {
  createCompanionProjection,
  isDesktopPreferences,
  setMainWindowPositionPreset,
} from "./desktop";

beforeEach(() => {
  mocks.invoke.mockReset();
});

describe("desktop command adapters", () => {
  it("passes a typed quick-position preset to the registered Tauri command", async () => {
    mocks.invoke.mockResolvedValue(undefined);

    await setMainWindowPositionPreset("bottom-right");

    expect(mocks.invoke).toHaveBeenCalledOnce();
    expect(mocks.invoke).toHaveBeenCalledWith(
      "set_main_window_position_preset",
      { preset: "bottom-right" },
    );
  });
});

describe("desktop preferences boundary", () => {
  it("accepts the complete persisted preference projection", () => {
    expect(isDesktopPreferences(DEFAULT_DESKTOP_PREFERENCES)).toBe(true);
  });

  it("rejects missing, mistyped, and unsupported preference values", () => {
    const missingLaunchAtLogin = Object.fromEntries(
      Object.entries(DEFAULT_DESKTOP_PREFERENCES).filter(
        ([key]) => key !== "launchAtLogin",
      ),
    );
    expect(isDesktopPreferences(missingLaunchAtLogin)).toBe(false);
    expect(
      isDesktopPreferences({
        ...DEFAULT_DESKTOP_PREFERENCES,
        launchAtLogin: "false",
      }),
    ).toBe(false);
    expect(
      isDesktopPreferences({
        ...DEFAULT_DESKTOP_PREFERENCES,
        clickThrough: "false",
      }),
    ).toBe(false);
    expect(
      isDesktopPreferences({
        ...DEFAULT_DESKTOP_PREFERENCES,
        opacityPercent: 50,
      }),
    ).toBe(false);
    expect(
      isDesktopPreferences({
        ...DEFAULT_DESKTOP_PREFERENCES,
        showMainWindow: undefined,
      }),
    ).toBe(false);
    expect(
      isDesktopPreferences({
        ...DEFAULT_DESKTOP_PREFERENCES,
        radarSource: "other",
      }),
    ).toBe(false);
  });
});

describe("companion projection", () => {
  it("projects the normalized leader without remote-schema parsing", () => {
    expect(createCompanionProjection(sampleSnapshot, "ready")).toEqual({
      modelName: "GPT-5.6 Sol",
      reasoningEffort: "max",
      scoreText: "106.3",
      tieCount: 0,
      statusLabel: "已同步",
    });
  });

  it("preserves extra tied leaders as a bounded count", () => {
    const tied = {
      ...sampleSnapshot,
      leaderIds: [sampleSnapshot.rankings[0].id, "gpt-5.7-pro:high"],
      rankings: [
        sampleSnapshot.rankings[0],
        {
          ...sampleSnapshot.rankings[0],
          id: "gpt-5.7-pro:high",
          label: "GPT-5.7 Pro high",
          model: "gpt-5.7-pro",
          reasoningEffort: "high",
        },
      ],
    };

    expect(createCompanionProjection(tied, "stale").tieCount).toBe(1);
    expect(createCompanionProjection(tied, "stale").statusLabel).toBe(
      "离线 / 旧数据",
    );
  });

  it("uses explicit empty values when no snapshot exists", () => {
    expect(createCompanionProjection(null, "booting")).toEqual({
      modelName: "暂无数据",
      reasoningEffort: "",
      scoreText: "--",
      tieCount: 0,
      statusLabel: "连接中",
    });
  });
});
