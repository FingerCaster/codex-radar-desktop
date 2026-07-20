import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import { openUrl } from "@tauri-apps/plugin-opener";

import type { RadarSnapshot, RadarSource } from "../types/radar";

const PRIMARY_SUMMARY_URL =
  "https://codex-reset-radar.pages.dev/current.json";
const LEGACY_CACHE_KEY = "model-radar:last-snapshot:v1";
const CACHE_KEYS: Record<RadarSource, string> = {
  main: "model-radar:last-snapshot:v2:main",
  distributed: "model-radar:last-snapshot:v2:distributed",
};
const SOURCE_SITE_URLS: Record<RadarSource, string> = {
  main: "https://codexradar.com",
  distributed: "https://deng.codexradar.com",
};
const SNAPSHOT_UPDATED_EVENT = "radar://snapshot-updated";
const REFRESH_FAILED_EVENT = "radar://refresh-failed";
const REFRESH_REQUESTED_EVENT = "radar://refresh-requested";

export interface RefreshOutcome {
  snapshot: RadarSnapshot;
  notModified: boolean;
  leaderChanged: boolean;
}

export interface RefreshFailure {
  source: RadarSource;
  kind: string;
  message: string;
  occurredAt: string;
}

export function isRadarSource(value: unknown): value is RadarSource {
  return value === "main" || value === "distributed";
}

export function isRadarSnapshot(value: unknown): value is RadarSnapshot {
  if (!isRecord(value)) {
    return false;
  }

  return isRadarSource(value.source) && hasRadarSnapshotFields(value);
}

export function loadCachedSnapshot(source: RadarSource): RadarSnapshot | null {
  if (typeof window === "undefined") {
    return null;
  }

  const cached = readCachedValue(CACHE_KEYS[source]);
  if (isRadarSnapshot(cached) && cached.source === source) {
    return cached;
  }

  if (source !== "main") {
    return null;
  }

  const legacy = readCachedValue(LEGACY_CACHE_KEY);
  if (!isLegacyMainSnapshot(legacy)) {
    return null;
  }

  const migrated: RadarSnapshot = { ...legacy, source: "main" };
  if (writeCachedSnapshot(migrated)) {
    try {
      window.localStorage.removeItem(LEGACY_CACHE_KEY);
    } catch {
      // A retained v1 entry is harmless once the source-specific cache exists.
    }
  }
  return migrated;
}

export function saveCachedSnapshot(snapshot: RadarSnapshot): void {
  writeCachedSnapshot(snapshot);
}

function writeCachedSnapshot(snapshot: RadarSnapshot): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  try {
    window.localStorage.setItem(
      CACHE_KEYS[snapshot.source],
      JSON.stringify(snapshot),
    );
    return true;
  } catch {
    // A denied or full storage area must not interrupt live state updates.
    return false;
  }
}

export async function getRadarSnapshot(): Promise<RadarSnapshot | null> {
  return invoke<RadarSnapshot | null>("get_radar_snapshot");
}

export async function refreshRadar(): Promise<RefreshOutcome> {
  return invoke<RefreshOutcome>("refresh_radar");
}

export async function setWindowExpanded(expanded: boolean): Promise<void> {
  await invoke("set_window_expanded", { expanded });
}

export async function hideWindow(): Promise<void> {
  await invoke("hide_window");
}

export function getSourceSiteUrl(source: RadarSource): string {
  return SOURCE_SITE_URLS[source];
}

export async function openSourceSite(source: RadarSource): Promise<void> {
  await openUrl(getSourceSiteUrl(source));
}

export async function ensureNotificationPermission(): Promise<boolean> {
  if (!isTauri()) {
    return false;
  }

  try {
    if (await isPermissionGranted()) {
      return true;
    }
    return (await requestPermission()) === "granted";
  } catch {
    return false;
  }
}

export function onSnapshotUpdated(
  handler: (snapshot: RadarSnapshot) => void,
): Promise<UnlistenFn> {
  return listen<RadarSnapshot>(SNAPSHOT_UPDATED_EVENT, (event) => {
    if (isRadarSnapshot(event.payload)) {
      handler(event.payload);
    }
  });
}

export function onRefreshFailed(
  handler: (failure: RefreshFailure) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(REFRESH_FAILED_EVENT, (event) => {
    if (isRefreshFailure(event.payload)) {
      handler(event.payload);
    }
  });
}

export function onRefreshRequested(handler: () => void): Promise<UnlistenFn> {
  return listen(REFRESH_REQUESTED_EVENT, handler);
}

export function isRefreshFailure(value: unknown): value is RefreshFailure {
  return (
    isRecord(value) &&
    isRadarSource(value.source) &&
    typeof value.kind === "string" &&
    typeof value.message === "string" &&
    typeof value.occurredAt === "string"
  );
}

export function normalizeRefreshFailure(
  value: unknown,
  fallbackSource: RadarSource,
): RefreshFailure {
  if (isRefreshFailure(value)) {
    return {
      source: value.source,
      kind: value.kind,
      message: value.message,
      occurredAt: value.occurredAt,
    };
  }

  return {
    source: fallbackSource,
    kind: "unknown",
    message: value instanceof Error ? value.message : String(value),
    occurredAt: new Date().toISOString(),
  };
}

export function isSnapshotOlder(
  candidate: RadarSnapshot,
  current: RadarSnapshot,
): boolean {
  return (
    candidate.source === current.source &&
    Date.parse(candidate.updatedAt) < Date.parse(current.updatedAt)
  );
}

type LegacyMainSnapshot = Omit<RadarSnapshot, "source">;

function isLegacyMainSnapshot(value: unknown): value is LegacyMainSnapshot {
  return (
    isRecord(value) &&
    value.source === undefined &&
    value.sourceUrl === PRIMARY_SUMMARY_URL &&
    hasRadarSnapshotFields(value)
  );
}

function hasRadarSnapshotFields(value: Record<string, unknown>): boolean {
  return (
    value.schemaVersion === "2.0" &&
    isTimestamp(value.updatedAt) &&
    isTimestamp(value.checkedAt) &&
    typeof value.sourceUrl === "string" &&
    Array.isArray(value.leaderIds) &&
    value.leaderIds.every((id) => typeof id === "string") &&
    Array.isArray(value.rankings) &&
    value.rankings.length > 0 &&
    value.rankings.every(isModelScore) &&
    isRecord(value.attribution) &&
    typeof value.attribution.text === "string" &&
    typeof value.attribution.url === "string"
  );
}

function readCachedValue(key: string): unknown {
  try {
    const raw = window.localStorage.getItem(key);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isModelScore(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    typeof value.label === "string" &&
    typeof value.model === "string" &&
    typeof value.reasoningEffort === "string" &&
    typeof value.score === "number" &&
    Number.isFinite(value.score) &&
    isNullableString(value.status) &&
    isNullableCount(value.passed) &&
    isNullableCount(value.tasks) &&
    isNullableCount(value.validTasks) &&
    isNullableNumber(value.averageCostUsd) &&
    isNullableNumber(value.averageTaskSeconds) &&
    isNullableString(value.averageTaskTimeHuman) &&
    isNullableString(value.wallTimeHuman)
  );
}

function isTimestamp(value: unknown): value is string {
  return typeof value === "string" && Number.isFinite(Date.parse(value));
}

function isNullableString(value: unknown): boolean {
  return value === null || typeof value === "string";
}

function isNullableNumber(value: unknown): boolean {
  return (
    value === null ||
    (typeof value === "number" && Number.isFinite(value) && value >= 0)
  );
}

function isNullableCount(value: unknown): boolean {
  return value === null || (isNullableNumber(value) && Number.isInteger(value));
}
