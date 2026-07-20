import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

import { getModelDisplayName } from "./model";
import { isRadarSource } from "./radar";
import {
  DESKTOP_OPACITY_VALUES,
  type CompanionProjection,
  type DesktopBooleanOption,
  type DesktopOpacityPercent,
  type DesktopPreferences,
  type MainWindowPositionPreset,
} from "../types/desktop";
import {
  RADAR_STATUS_LABELS,
  type RadarSnapshot,
  type RadarSource,
  type RadarStatus,
} from "../types/radar";

const PREFERENCES_UPDATED_EVENT = "desktop://preferences-updated";
const MAIN_EXPANDED_EVENT = "desktop://main-expanded";
const SHOW_MAIN_DETAILS_EVENT = "desktop://show-main-details";

const scoreFormatter = new Intl.NumberFormat("zh-CN", {
  maximumFractionDigits: 2,
});

export function isDesktopPreferences(
  value: unknown,
): value is DesktopPreferences {
  return (
    isRecord(value) &&
    typeof value.alwaysOnTop === "boolean" &&
    typeof value.clickThrough === "boolean" &&
    typeof value.positionLocked === "boolean" &&
    typeof value.showTaskbarWindow === "boolean" &&
    typeof value.showMainWindow === "boolean" &&
    typeof value.launchAtLogin === "boolean" &&
    isDesktopOpacityPercent(value.opacityPercent) &&
    isRadarSource(value.radarSource)
  );
}

export function createCompanionProjection(
  snapshot: RadarSnapshot | null,
  status: RadarStatus,
): CompanionProjection {
  const rankings = snapshot?.rankings ?? [];
  const leaderIds = new Set(snapshot?.leaderIds ?? []);
  const matches = rankings.filter((entry) => leaderIds.has(entry.id));
  const leaders = matches.length > 0 ? matches : rankings.slice(0, 1);
  const primary = leaders[0] ?? null;

  return {
    modelName: primary ? getModelDisplayName(primary) : "暂无数据",
    reasoningEffort: primary?.reasoningEffort ?? "",
    scoreText: primary ? scoreFormatter.format(primary.score) : "--",
    tieCount: Math.max(0, leaders.length - 1),
    statusLabel: RADAR_STATUS_LABELS[status],
  };
}

export function getCurrentWebviewLabel(): string {
  if (!isTauri()) {
    return "main";
  }

  return getCurrentWebviewWindow().label;
}

export async function getDesktopPreferences(): Promise<DesktopPreferences> {
  return requireDesktopPreferences(await invoke<unknown>("get_desktop_preferences"));
}

export async function setDesktopOption(
  option: DesktopBooleanOption,
  enabled: boolean,
): Promise<DesktopPreferences> {
  return requireDesktopPreferences(
    await invoke<unknown>("set_desktop_option", { option, enabled }),
  );
}

export async function setDesktopOpacity(
  opacityPercent: DesktopOpacityPercent,
): Promise<DesktopPreferences> {
  return requireDesktopPreferences(
    await invoke<unknown>("set_desktop_opacity", { opacityPercent }),
  );
}

export async function setDesktopRadarSource(
  source: RadarSource,
): Promise<DesktopPreferences> {
  return requireDesktopPreferences(
    await invoke<unknown>("set_desktop_radar_source", { source }),
  );
}

export async function setMainWindowPositionPreset(
  preset: MainWindowPositionPreset,
): Promise<void> {
  await invoke("set_main_window_position_preset", { preset });
}

export async function showMainDetails(): Promise<void> {
  await invoke("show_main_details");
}

export async function getMainExpanded(): Promise<boolean> {
  const expanded = await invoke<unknown>("get_main_expanded");
  if (typeof expanded !== "boolean") {
    throw new Error("invalid main expanded-state payload");
  }
  return expanded;
}

export async function showDesktopContextMenu(): Promise<void> {
  await invoke("show_desktop_context_menu");
}

export async function updateCompanionProjection(
  projection: CompanionProjection,
): Promise<void> {
  await invoke("update_companion_projection", { projection });
}

export function onDesktopPreferencesUpdated(
  handler: (preferences: DesktopPreferences) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(PREFERENCES_UPDATED_EVENT, (event) => {
    if (isDesktopPreferences(event.payload)) {
      handler(event.payload);
    }
  });
}

export function onMainExpanded(
  handler: (expanded: boolean) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(MAIN_EXPANDED_EVENT, (event) => {
    if (typeof event.payload === "boolean") {
      handler(event.payload);
    }
  });
}

export function onShowMainDetails(handler: () => void): Promise<UnlistenFn> {
  return listen(SHOW_MAIN_DETAILS_EVENT, () => handler());
}

function requireDesktopPreferences(value: unknown): DesktopPreferences {
  if (!isDesktopPreferences(value)) {
    throw new Error("invalid desktop preferences payload");
  }
  return value;
}

function isDesktopOpacityPercent(
  value: unknown,
): value is DesktopOpacityPercent {
  return (
    typeof value === "number" &&
    DESKTOP_OPACITY_VALUES.some((allowed) => allowed === value)
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
