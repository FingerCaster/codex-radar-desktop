import { useCallback, useEffect, useReducer } from "react";

import {
  getDesktopPreferences,
  onDesktopPreferencesUpdated,
  setDesktopOpacity,
  setDesktopOption,
} from "../lib/desktop";
import {
  DEFAULT_DESKTOP_PREFERENCES,
  type DesktopBooleanOption,
  type DesktopOpacityPercent,
  type DesktopPreferences,
} from "../types/desktop";

export interface DesktopPreferencesViewState {
  preferences: DesktopPreferences;
  radarActivationEpoch: number;
  hydrated: boolean;
  error: string | null;
}

export type DesktopPreferencesAction =
  | { type: "preferences-received"; preferences: DesktopPreferences }
  | { type: "preferences-failed"; message: string };

export function createInitialDesktopPreferencesState(): DesktopPreferencesViewState {
  return {
    preferences: { ...DEFAULT_DESKTOP_PREFERENCES },
    radarActivationEpoch: 0,
    hydrated: false,
    error: null,
  };
}

export function desktopPreferencesReducer(
  state: DesktopPreferencesViewState,
  action: DesktopPreferencesAction,
): DesktopPreferencesViewState {
  switch (action.type) {
    case "preferences-received":
      return {
        preferences: action.preferences,
        radarActivationEpoch:
          state.radarActivationEpoch +
          Number(
            action.preferences.radarSource !== state.preferences.radarSource,
          ),
        hydrated: true,
        error: null,
      };
    case "preferences-failed":
      return {
        ...state,
        hydrated: true,
        error: action.message,
      };
  }
}

export function useDesktopPreferences() {
  const [state, dispatch] = useReducer(
    desktopPreferencesReducer,
    undefined,
    createInitialDesktopPreferencesState,
  );

  const setOption = useCallback(
    async (option: DesktopBooleanOption, enabled: boolean) => {
      try {
        const preferences = await setDesktopOption(option, enabled);
        dispatch({ type: "preferences-received", preferences });
        return preferences;
      } catch (error) {
        dispatch({ type: "preferences-failed", message: errorMessage(error) });
        throw error;
      }
    },
    [],
  );

  const setOpacity = useCallback(async (opacity: DesktopOpacityPercent) => {
    try {
      const preferences = await setDesktopOpacity(opacity);
      dispatch({ type: "preferences-received", preferences });
      return preferences;
    } catch (error) {
      dispatch({ type: "preferences-failed", message: errorMessage(error) });
      throw error;
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    let receivedUpdateDuringHydration = false;
    const unlisteners: Array<() => void> = [];

    const initialize = async () => {
      try {
        const unlisten = await onDesktopPreferencesUpdated((preferences) => {
          if (!disposed) {
            receivedUpdateDuringHydration = true;
            dispatch({ type: "preferences-received", preferences });
          }
        });
        if (disposed) {
          unlisten();
          return;
        }

        unlisteners.push(unlisten);
      } catch (error) {
        if (!disposed) {
          dispatch({
            type: "preferences-failed",
            message: errorMessage(error),
          });
        }
        return;
      }

      try {
        const preferences = await getDesktopPreferences();
        if (!disposed && !receivedUpdateDuringHydration) {
          dispatch({ type: "preferences-received", preferences });
        }
      } catch (error) {
        if (!disposed && !receivedUpdateDuringHydration) {
          dispatch({
            type: "preferences-failed",
            message: errorMessage(error),
          });
        }
      }
    };

    void initialize();

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  return { ...state, setOption, setOpacity };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
