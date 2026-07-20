import type { RadarSource, RadarStatus } from "./radar";

export const DESKTOP_OPACITY_VALUES = [100, 90, 80, 70, 60] as const;

export type DesktopOpacityPercent =
  (typeof DESKTOP_OPACITY_VALUES)[number];

export type DesktopBooleanOption =
  | "alwaysOnTop"
  | "clickThrough"
  | "positionLocked"
  | "showTaskbarWindow"
  | "showMainWindow"
  | "launchAtLogin";

export interface DesktopPreferences {
  alwaysOnTop: boolean;
  clickThrough: boolean;
  positionLocked: boolean;
  showTaskbarWindow: boolean;
  showMainWindow: boolean;
  launchAtLogin: boolean;
  opacityPercent: DesktopOpacityPercent;
  radarSource: RadarSource;
}

export const DEFAULT_DESKTOP_PREFERENCES: Readonly<DesktopPreferences> = {
  alwaysOnTop: true,
  clickThrough: false,
  positionLocked: false,
  showTaskbarWindow: true,
  showMainWindow: true,
  launchAtLogin: false,
  opacityPercent: 100,
  radarSource: "main",
};

export interface CompanionProjection {
  modelName: string;
  reasoningEffort: string;
  scoreText: string;
  tieCount: number;
  statusLabel: string;
}

export type DesktopAction = () => void | Promise<void>;

export type DesktopSettingsPending =
  | DesktopBooleanOption
  | "radarSource"
  | "opacityPercent";

export interface SettingsViewProps {
  preferences: DesktopPreferences;
  pending: DesktopSettingsPending | null;
  error: string | null;
  onBack: DesktopAction;
  onSetOption: (
    option: DesktopBooleanOption,
    enabled: boolean,
  ) => void | Promise<void>;
  onSetOpacity: (opacity: DesktopOpacityPercent) => void | Promise<void>;
  onSetRadarSource: (source: RadarSource) => void | Promise<void>;
}

export interface TaskbarViewProps {
  projection: CompanionProjection;
  status: RadarStatus;
  onShowDetails: DesktopAction;
  onOpenContextMenu: DesktopAction;
}
