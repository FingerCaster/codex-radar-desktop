import {
  type CSSProperties,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { CompactView } from "./components/CompactView";
import { DetailView } from "./components/DetailView";
import { SettingsView } from "./components/SettingsView";
import { TaskbarView } from "./components/TaskbarView";
import { useDesktopPreferences } from "./hooks/useDesktopPreferences";
import { useRadar } from "./hooks/useRadar";
import {
  createCompanionProjection,
  getCurrentWebviewLabel,
  getMainExpanded,
  onMainExpanded,
  onShowMainDetails,
  setMainWindowPositionPreset,
  showDesktopContextMenu,
  showMainDetails,
  updateCompanionProjection,
} from "./lib/desktop";
import {
  hideWindow,
  openSourceSite,
  setWindowExpanded,
  type RefreshFailure,
} from "./lib/radar";
import type {
  DesktopBooleanOption,
  DesktopOpacityPercent,
  DesktopSettingsPending,
  MainWindowPositionPreset,
} from "./types/desktop";
import type { RadarSource } from "./types/radar";
import "./App.css";

type WindowOpacityStyle = CSSProperties & {
  "--window-opacity": number;
};

type RadarView = "compact" | "detail";
type MainView = RadarView | "settings";

function userFacingError(failure: RefreshFailure | null): string | null {
  if (!failure) {
    return null;
  }

  switch (failure.kind) {
    case "network":
      return "连接失败，正在显示上次同步的数据";
    case "http":
      return "数据源暂时不可用";
    case "json":
    case "schema":
    case "type":
    case "timestamp":
    case "no_candidates":
      return "数据格式暂时无法识别";
    case "stale_payload":
      return "已忽略早于当前记录的数据";
    default:
      return "暂时无法完成刷新";
  }
}

function App() {
  const webviewLabel = useMemo(() => getCurrentWebviewLabel(), []);
  const isTaskbarWindow = webviewLabel === "taskbar";
  const desktop = useDesktopPreferences();
  const {
    setOpacity: updateDesktopOpacity,
    setOption: updateDesktopOption,
    setRadarSource: updateDesktopRadarSource,
  } = desktop;
  const radar = useRadar({
    passive: isTaskbarWindow,
    source: desktop.preferences.radarSource,
    activationEpoch: desktop.radarActivationEpoch,
    enabled: desktop.hydrated,
  });
  const [mainView, setMainView] = useState<MainView>("compact");
  const [settingsOrigin, setSettingsOrigin] = useState<RadarView>("compact");
  const [settingsPending, setSettingsPending] =
    useState<DesktopSettingsPending | null>(null);
  const settingsPendingRef = useRef<DesktopSettingsPending | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [windowError, setWindowError] = useState<string | null>(null);

  const projection = useMemo(
    () => createCompanionProjection(radar.snapshot, radar.status),
    [radar.snapshot, radar.status],
  );
  const opacityStyle = useMemo<WindowOpacityStyle>(
    () => ({
      "--window-opacity": desktop.preferences.opacityPercent / 100,
    }),
    [desktop.preferences.opacityPercent],
  );
  const error = useMemo(
    () => windowError ?? userFacingError(radar.error),
    [radar.error, windowError],
  );

  const resizeMainWindow = useCallback(async (nextExpanded: boolean) => {
    try {
      await setWindowExpanded(nextExpanded);
      setWindowError(null);
      return true;
    } catch {
      setWindowError("窗口尺寸调整失败，请重试");
      return false;
    }
  }, []);

  const changeRadarView = useCallback(
    async (nextView: RadarView) => {
      if (await resizeMainWindow(nextView === "detail")) {
        setMainView(nextView);
      }
    },
    [resizeMainWindow],
  );
  const expand = useCallback(() => changeRadarView("detail"), [changeRadarView]);
  const collapse = useCallback(() => changeRadarView("compact"), [changeRadarView]);
  const openSettings = useCallback(async () => {
    if (mainView === "settings") {
      return;
    }

    if (mainView === "compact" && !(await resizeMainWindow(true))) {
      return;
    }

    setSettingsOrigin(mainView);
    setSettingsError(null);
    setWindowError(null);
    setMainView("settings");
  }, [mainView, resizeMainWindow]);
  const closeSettings = useCallback(async () => {
    if (settingsPendingRef.current !== null) {
      return;
    }
    if (settingsOrigin === "compact" && !(await resizeMainWindow(false))) {
      setSettingsError("窗口尺寸调整失败，请重试");
      return;
    }

    setSettingsError(null);
    setMainView(settingsOrigin);
  }, [resizeMainWindow, settingsOrigin]);
  const runSettingsUpdate = useCallback(
    async (
      pending: DesktopSettingsPending,
      update: () => Promise<unknown>,
    ) => {
      if (settingsPendingRef.current !== null) {
        return;
      }

      settingsPendingRef.current = pending;
      setSettingsPending(pending);
      setSettingsError(null);
      try {
        await update();
      } catch {
        setSettingsError("设置保存失败，请重试");
      } finally {
        settingsPendingRef.current = null;
        setSettingsPending(null);
      }
    },
    [],
  );
  const setSettingsOption = useCallback(
    (option: DesktopBooleanOption, enabled: boolean) =>
      runSettingsUpdate(option, () => updateDesktopOption(option, enabled)),
    [runSettingsUpdate, updateDesktopOption],
  );
  const setSettingsOpacity = useCallback(
    (opacity: DesktopOpacityPercent) =>
      runSettingsUpdate("opacityPercent", () => updateDesktopOpacity(opacity)),
    [runSettingsUpdate, updateDesktopOpacity],
  );
  const setSettingsRadarSource = useCallback(
    (source: RadarSource) =>
      runSettingsUpdate("radarSource", () => updateDesktopRadarSource(source)),
    [runSettingsUpdate, updateDesktopRadarSource],
  );
  const setSettingsPositionPreset = useCallback(
    (preset: MainWindowPositionPreset) =>
      runSettingsUpdate("positionPreset", () =>
        setMainWindowPositionPreset(preset),
      ),
    [runSettingsUpdate],
  );
  const openSelectedSource = useCallback(
    () => openSourceSite(desktop.preferences.radarSource),
    [desktop.preferences.radarSource],
  );

  useEffect(() => {
    if (isTaskbarWindow) {
      return;
    }

    let disposed = false;
    let receivedExpandedDuringHydration = false;
    const unlisteners: Array<() => void> = [];

    const registerListener = async (
      register: () => Promise<() => void>,
    ): Promise<boolean> => {
      try {
        const unlisten = await register();
        if (disposed) {
          unlisten();
        } else {
          unlisteners.push(unlisten);
        }
        return true;
      } catch {
        return false;
      }
    };

    const initialize = async () => {
      const [expandedRegistered, detailsRegistered] = await Promise.all([
        registerListener(() =>
          onMainExpanded((nextExpanded) => {
            if (!disposed) {
              receivedExpandedDuringHydration = true;
              setMainView((current) =>
                current === "settings"
                  ? current
                  : nextExpanded
                    ? "detail"
                    : "compact",
              );
              setWindowError(null);
            }
          }),
        ),
        registerListener(() =>
          onShowMainDetails(() => {
            if (!disposed) {
              receivedExpandedDuringHydration = true;
              setSettingsError(null);
              setMainView("detail");
              setWindowError(null);
            }
          }),
        ),
      ]);

      if (disposed) {
        return;
      }

      if (!expandedRegistered || !detailsRegistered) {
        if (!disposed && !receivedExpandedDuringHydration) {
          setWindowError("窗口状态同步失败，请重试");
        }
        return;
      }

      try {
        const currentExpanded = await getMainExpanded();
        if (!disposed && !receivedExpandedDuringHydration) {
          setMainView(currentExpanded ? "detail" : "compact");
          setWindowError(null);
        }
      } catch {
        if (!disposed && !receivedExpandedDuringHydration) {
          setWindowError("窗口状态同步失败，请重试");
        }
      }
    };

    void initialize();

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [isTaskbarWindow]);

  useEffect(() => {
    if (!isTaskbarWindow) {
      void updateCompanionProjection(projection).catch(() => undefined);
    }
  }, [isTaskbarWindow, projection]);

  useEffect(() => {
    if (isTaskbarWindow) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") {
        return;
      }
      if (mainView === "settings") {
        event.preventDefault();
        void closeSettings();
      } else if (mainView === "detail") {
        event.preventDefault();
        void collapse();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [closeSettings, collapse, isTaskbarWindow, mainView]);

  if (isTaskbarWindow) {
    return (
      <div className="window-opacity-root taskbar-window-root" style={opacityStyle}>
        <TaskbarView
          onOpenContextMenu={showDesktopContextMenu}
          onShowDetails={showMainDetails}
          projection={projection}
          status={radar.status}
        />
      </div>
    );
  }

  const sharedProps = {
    snapshot: radar.snapshot,
    status: radar.status,
    error,
    positionLocked: desktop.preferences.positionLocked,
    onRefresh: radar.refresh,
    onHide: hideWindow,
  };

  return (
    <div className="window-opacity-root main-window-root" style={opacityStyle}>
      {mainView === "settings" ? (
        <SettingsView
          error={settingsError ?? windowError}
          onBack={closeSettings}
          onSetOpacity={setSettingsOpacity}
          onSetOption={setSettingsOption}
          onSetPositionPreset={setSettingsPositionPreset}
          onSetRadarSource={setSettingsRadarSource}
          pending={settingsPending}
          preferences={desktop.preferences}
        />
      ) : mainView === "detail" ? (
        <DetailView
          {...sharedProps}
          onCollapse={collapse}
          onOpenSettings={openSettings}
          onOpenSource={openSelectedSource}
        />
      ) : (
        <CompactView
          {...sharedProps}
          onExpand={expand}
          onOpenSettings={openSettings}
        />
      )}
    </div>
  );
}

export default App;
