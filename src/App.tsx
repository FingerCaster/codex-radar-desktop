import {
  type CSSProperties,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";

import { CompactView } from "./components/CompactView";
import { DetailView } from "./components/DetailView";
import { TaskbarView } from "./components/TaskbarView";
import { useDesktopPreferences } from "./hooks/useDesktopPreferences";
import { useRadar } from "./hooks/useRadar";
import {
  createCompanionProjection,
  getCurrentWebviewLabel,
  getMainExpanded,
  onMainExpanded,
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
import "./App.css";

type WindowOpacityStyle = CSSProperties & {
  "--window-opacity": number;
};

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
  const radar = useRadar({
    passive: isTaskbarWindow,
    source: desktop.preferences.radarSource,
    activationEpoch: desktop.radarActivationEpoch,
    enabled: desktop.hydrated,
  });
  const [expanded, setExpanded] = useState(false);
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

  const changeWindowMode = useCallback(async (nextExpanded: boolean) => {
    try {
      await setWindowExpanded(nextExpanded);
      setExpanded(nextExpanded);
      setWindowError(null);
    } catch {
      setWindowError("窗口尺寸调整失败，请重试");
    }
  }, []);

  const expand = useCallback(
    () => changeWindowMode(true),
    [changeWindowMode],
  );
  const collapse = useCallback(
    () => changeWindowMode(false),
    [changeWindowMode],
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
    let unlisten: (() => void) | undefined;

    void onMainExpanded((nextExpanded) => {
      if (!disposed) {
        receivedExpandedDuringHydration = true;
        setExpanded(nextExpanded);
        setWindowError(null);
      }
    })
      .then(async (registeredUnlisten) => {
        if (disposed) {
          registeredUnlisten();
        } else {
          unlisten = registeredUnlisten;
          const currentExpanded = await getMainExpanded();
          if (!disposed && !receivedExpandedDuringHydration) {
            setExpanded(currentExpanded);
            setWindowError(null);
          }
        }
      })
      .catch(() => {
        if (!disposed && !receivedExpandedDuringHydration) {
          setWindowError("窗口状态同步失败，请重试");
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
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
      if (event.key === "Escape" && expanded) {
        event.preventDefault();
        void collapse();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [collapse, expanded, isTaskbarWindow]);

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
      {expanded ? (
        <DetailView
          {...sharedProps}
          onCollapse={collapse}
          onOpenSource={openSelectedSource}
        />
      ) : (
        <CompactView {...sharedProps} onExpand={expand} />
      )}
    </div>
  );
}

export default App;
