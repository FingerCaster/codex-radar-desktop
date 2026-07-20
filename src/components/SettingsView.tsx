import { ArrowLeft, Settings2 } from "lucide-react";

import { IconButton } from "./IconButton";
import {
  DESKTOP_OPACITY_VALUES,
  type DesktopBooleanOption,
  type SettingsViewProps,
} from "../types/desktop";
import type { RadarSource } from "../types/radar";

const BOOLEAN_SETTINGS: ReadonlyArray<{
  option: DesktopBooleanOption;
  label: string;
}> = [
  {
    option: "alwaysOnTop",
    label: "总是置顶",
  },
  {
    option: "clickThrough",
    label: "鼠标穿透",
  },
  {
    option: "positionLocked",
    label: "锁定窗口位置",
  },
  {
    option: "showTaskbarWindow",
    label: "显示任务栏窗口",
  },
  {
    option: "showMainWindow",
    label: "显示主窗口",
  },
];

const RADAR_SOURCES: ReadonlyArray<{ value: RadarSource; label: string }> = [
  { value: "main", label: "主站" },
  { value: "distributed", label: "分布式" },
];

export function SettingsView({
  preferences,
  pending,
  error,
  onBack,
  onSetOption,
  onSetOpacity,
  onSetRadarSource,
}: SettingsViewProps) {
  const disabled = pending !== null;
  const dragRegion = preferences.positionLocked ? undefined : true;

  return (
    <section
      aria-busy={disabled}
      aria-label="Codex Radar 设置"
      className="radar-shell settings-view"
      data-state={disabled ? "saving" : error ? "error" : "ready"}
    >
      <header className="settings-header" data-tauri-drag-region={dragRegion}>
        <IconButton
          disabled={disabled}
          icon={ArrowLeft}
          label="返回雷达"
          onClick={onBack}
        />
        <div className="settings-title" data-tauri-drag-region={dragRegion}>
          <Settings2 aria-hidden="true" size={16} strokeWidth={2} />
          <h1 data-tauri-drag-region={dragRegion}>设置</h1>
        </div>
        <span aria-hidden="true" className="settings-header-spacer" />
      </header>

      <main className="settings-content">
        <section className="settings-section" aria-labelledby="settings-source-title">
          <div className="settings-section-heading">
            <h2 id="settings-source-title">雷达数据源</h2>
          </div>
          <div aria-label="雷达数据源" className="settings-segmented" role="group">
            {RADAR_SOURCES.map((source) => (
              <button
                aria-pressed={preferences.radarSource === source.value}
                disabled={disabled}
                key={source.value}
                onClick={() => onSetRadarSource(source.value)}
                type="button"
              >
                {source.label}
              </button>
            ))}
          </div>
        </section>

        <section className="settings-section settings-options" aria-labelledby="settings-window-title">
          <div className="settings-section-heading">
            <h2 id="settings-window-title">窗口</h2>
          </div>
          {BOOLEAN_SETTINGS.map((setting) => {
            const inputId = `desktop-setting-${setting.option}`;
            return (
              <label className="settings-option" htmlFor={inputId} key={setting.option}>
                <span className="settings-option-copy">
                  <strong>{setting.label}</strong>
                </span>
                <input
                  checked={preferences[setting.option]}
                  disabled={disabled}
                  id={inputId}
                  onChange={(event) =>
                    onSetOption(setting.option, event.currentTarget.checked)
                  }
                  type="checkbox"
                />
              </label>
            );
          })}
        </section>

        <section className="settings-section" aria-labelledby="settings-opacity-title">
          <div className="settings-section-heading">
            <h2 id="settings-opacity-title">窗口不透明度</h2>
          </div>
          <div aria-label="窗口不透明度" className="settings-segmented settings-opacity" role="group">
            {DESKTOP_OPACITY_VALUES.map((opacity) => (
              <button
                aria-label={`${opacity}%`}
                aria-pressed={preferences.opacityPercent === opacity}
                disabled={disabled}
                key={opacity}
                onClick={() => onSetOpacity(opacity)}
                type="button"
              >
                {opacity}
              </button>
            ))}
          </div>
        </section>

        <section className="settings-section settings-system" aria-labelledby="settings-system-title">
          <div className="settings-section-heading">
            <h2 id="settings-system-title">系统</h2>
          </div>
          <label className="settings-option" htmlFor="desktop-setting-launchAtLogin">
            <span className="settings-option-copy">
              <strong>开机自启</strong>
            </span>
            <input
              checked={preferences.launchAtLogin}
              disabled={disabled}
              id="desktop-setting-launchAtLogin"
              onChange={(event) =>
                onSetOption("launchAtLogin", event.currentTarget.checked)
              }
              type="checkbox"
            />
          </label>
        </section>

        <div className="settings-feedback">
          {disabled ? <span role="status">正在保存设置</span> : null}
          {!disabled && error ? <span role="alert">{error}</span> : null}
        </div>
      </main>
    </section>
  );
}

export default SettingsView;
