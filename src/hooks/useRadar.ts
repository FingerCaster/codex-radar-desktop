import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useReducer,
  useRef,
} from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";

import type {
  RadarSnapshot,
  RadarSource,
  RadarStatus,
} from "../types/radar";
import {
  ensureNotificationPermission,
  getRadarSnapshot,
  isSnapshotOlder,
  loadCachedSnapshot,
  normalizeRefreshFailure,
  onRefreshFailed,
  onRefreshRequested,
  onSnapshotUpdated,
  refreshRadar,
  saveCachedSnapshot,
  type RefreshFailure,
} from "../lib/radar";

export interface RadarViewState {
  source: RadarSource;
  activationEpoch: number;
  snapshot: RadarSnapshot | null;
  status: RadarStatus;
  error: RefreshFailure | null;
  notificationsEnabled: boolean;
}

export type RadarAction =
  | {
      type: "source-selected";
      source: RadarSource;
      activationEpoch: number;
      cached: RadarSnapshot | null;
    }
  | { type: "refresh-started"; source: RadarSource }
  | {
      type: "snapshot-received";
      source: RadarSource;
      snapshot: RadarSnapshot;
    }
  | {
      type: "refresh-failed";
      source: RadarSource;
      failure: RefreshFailure;
    }
  | { type: "permission-resolved"; enabled: boolean };

export function createInitialRadarState(
  source: RadarSource,
  cached: RadarSnapshot | null = null,
  activationEpoch = 0,
): RadarViewState {
  const matchingCache = cached?.source === source ? cached : null;
  return {
    source,
    activationEpoch,
    snapshot: matchingCache,
    status: matchingCache ? "stale" : "booting",
    error: null,
    notificationsEnabled: false,
  };
}

export function radarReducer(
  state: RadarViewState,
  action: RadarAction,
): RadarViewState {
  switch (action.type) {
    case "source-selected": {
      if (
        state.source === action.source &&
        state.activationEpoch === action.activationEpoch &&
        state.snapshot?.source === action.source
      ) {
        return state;
      }

      const next = createInitialRadarState(
        action.source,
        action.cached,
        action.activationEpoch,
      );
      return {
        ...next,
        notificationsEnabled: state.notificationsEnabled,
      };
    }
    case "refresh-started":
      if (action.source !== state.source) {
        return state;
      }
      return {
        ...state,
        status: state.snapshot ? "refreshing" : "booting",
        error: null,
      };
    case "snapshot-received":
      if (
        action.source !== state.source ||
        action.snapshot.source !== action.source
      ) {
        return state;
      }
      if (state.snapshot && isSnapshotOlder(action.snapshot, state.snapshot)) {
        return {
          ...state,
          status: "stale",
          error: {
            source: action.source,
            kind: "stale_payload",
            message: "ignored a snapshot older than the last-known-good data",
            occurredAt: action.snapshot.checkedAt,
          },
        };
      }
      return {
        ...state,
        snapshot: action.snapshot,
        status: "ready",
        error: null,
      };
    case "refresh-failed":
      if (action.failure.kind === "superseded") {
        return state;
      }
      if (
        action.source !== state.source ||
        action.failure.source !== action.source
      ) {
        return state;
      }
      return {
        ...state,
        status: state.snapshot ? "stale" : "unavailable",
        error: action.failure,
      };
    case "permission-resolved":
      return { ...state, notificationsEnabled: action.enabled };
  }
}

export interface UseRadarOptions {
  passive?: boolean;
  source?: RadarSource;
  activationEpoch?: number;
  enabled?: boolean;
}

export function useRadar({
  passive = false,
  source = "main",
  activationEpoch = 0,
  enabled = true,
}: UseRadarOptions = {}) {
  const [state, dispatch] = useReducer(
    radarReducer,
    undefined,
    () =>
      createInitialRadarState(
        source,
        enabled ? loadCachedSnapshot(source) : null,
        activationEpoch,
      ),
  );

  const activationRef = useRef({ source, epoch: activationEpoch });

  const refresh = useCallback(async () => {
    if (!enabled) {
      return;
    }

    const requestedActivation = activationRef.current;
    const requestedSource = requestedActivation.source;
    dispatch({ type: "refresh-started", source: requestedSource });
    try {
      const outcome = await refreshRadar();
      if (activationRef.current !== requestedActivation) {
        return;
      }
      dispatch({
        type: "snapshot-received",
        source: requestedSource,
        snapshot: outcome.snapshot,
      });
    } catch (error) {
      if (activationRef.current !== requestedActivation) {
        return;
      }
      dispatch({
        type: "refresh-failed",
        source: requestedSource,
        failure: normalizeRefreshFailure(error, requestedSource),
      });
    }
  }, [enabled]);
  const sourceRef = useRef(source);
  const refreshRef = useRef(refresh);

  useLayoutEffect(() => {
    if (
      activationRef.current.source !== source ||
      activationRef.current.epoch !== activationEpoch
    ) {
      activationRef.current = { source, epoch: activationEpoch };
    }
    sourceRef.current = source;
    refreshRef.current = refresh;
  }, [activationEpoch, refresh, source]);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    dispatch({
      type: "source-selected",
      source,
      activationEpoch,
      cached: loadCachedSnapshot(source),
    });
  }, [activationEpoch, enabled, source]);

  useEffect(() => {
    if (
      enabled &&
      state.source === source &&
      state.activationEpoch === activationEpoch &&
      state.snapshot?.source === source
    ) {
      saveCachedSnapshot(state.snapshot);
    }
  }, [
    activationEpoch,
    enabled,
    source,
    state.activationEpoch,
    state.snapshot,
    state.source,
  ]);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    const trackListener = async (registration: Promise<UnlistenFn>) => {
      try {
        const unlisten = await registration;
        if (disposed) {
          unlisten();
        } else {
          unlisteners.push(unlisten);
        }
      } catch (error) {
        if (!disposed) {
          const currentSource = sourceRef.current;
          dispatch({
            type: "refresh-failed",
            source: currentSource,
            failure: {
              source: currentSource,
              kind: "listener",
              message: error instanceof Error ? error.message : String(error),
              occurredAt: new Date().toISOString(),
            },
          });
        }
      }
    };

    void trackListener(
      onSnapshotUpdated((snapshot) => {
        dispatch({
          type: "snapshot-received",
          source: snapshot.source,
          snapshot,
        });
      }),
    );
    void trackListener(
      onRefreshFailed((failure) => {
        dispatch({
          type: "refresh-failed",
          source: failure.source,
          failure,
        });
      }),
    );
    if (!passive) {
      void trackListener(
        onRefreshRequested(() => {
          void refreshRef.current();
        }),
      );
    }

    const handleOnline = () => void refreshRef.current();
    if (!passive) {
      window.addEventListener("online", handleOnline);
    }

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
      if (!passive) {
        window.removeEventListener("online", handleOnline);
      }
    };
  }, [enabled, passive]);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    let disposed = false;
    const requestedActivation = activationRef.current;
    void getRadarSnapshot()
      .then((snapshot) => {
        if (
          disposed ||
          activationRef.current !== requestedActivation
        ) {
          return;
        }
        if (snapshot) {
          dispatch({
            type: "snapshot-received",
            source: snapshot.source,
            snapshot,
          });
        } else if (!passive) {
          void refresh();
        }
      })
      .catch((error) => {
        if (
          !disposed &&
          activationRef.current === requestedActivation
        ) {
          dispatch({
            type: "refresh-failed",
            source: requestedActivation.source,
            failure: normalizeRefreshFailure(
              error,
              requestedActivation.source,
            ),
          });
        }
      });

    return () => {
      disposed = true;
    };
  }, [activationEpoch, enabled, passive, refresh, source]);

  useEffect(() => {
    if (!enabled || passive) {
      return;
    }

    let disposed = false;
    void ensureNotificationPermission().then((notificationsEnabled) => {
      if (!disposed) {
        dispatch({
          type: "permission-resolved",
          enabled: notificationsEnabled,
        });
      }
    });

    return () => {
      disposed = true;
    };
  }, [enabled, passive]);

  const visibleState =
    enabled &&
    state.source === source &&
    state.activationEpoch === activationEpoch
      ? state
      : {
          ...createInitialRadarState(source, null, activationEpoch),
          notificationsEnabled: state.notificationsEnabled,
        };

  return { ...visibleState, refresh };
}
