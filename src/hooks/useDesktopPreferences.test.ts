import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  getDesktopPreferences,
  onDesktopPreferencesUpdated,
  setDesktopRadarSource,
} from "../lib/desktop";
import { DEFAULT_DESKTOP_PREFERENCES } from "../types/desktop";
import type { DesktopPreferences } from "../types/desktop";
import {
  createInitialDesktopPreferencesState,
  desktopPreferencesReducer,
  useDesktopPreferences,
} from "./useDesktopPreferences";

vi.mock("../lib/desktop", () => ({
  getDesktopPreferences: vi.fn(),
  onDesktopPreferencesUpdated: vi.fn(),
  setDesktopOpacity: vi.fn(),
  setDesktopOption: vi.fn(),
  setDesktopRadarSource: vi.fn(),
}));

describe("desktopPreferencesReducer", () => {
  it("starts with safe visible defaults until Rust hydrates state", () => {
    const state = createInitialDesktopPreferencesState();
    expect(state.hydrated).toBe(false);
    expect(state.preferences.showMainWindow).toBe(true);
    expect(state.preferences.showTaskbarWindow).toBe(true);
    expect(state.preferences.opacityPercent).toBe(100);
    expect(state.preferences.radarSource).toBe("main");
    expect(state.radarActivationEpoch).toBe(0);
  });

  it("commits a complete accepted native projection", () => {
    const preferences = {
      ...DEFAULT_DESKTOP_PREFERENCES,
      positionLocked: true,
      opacityPercent: 80 as const,
    };
    const state = desktopPreferencesReducer(
      createInitialDesktopPreferencesState(),
      { type: "preferences-received", preferences },
    );

    expect(state.preferences).toBe(preferences);
    expect(state.radarActivationEpoch).toBe(0);
    expect(state.hydrated).toBe(true);
    expect(state.error).toBeNull();
  });

  it("increments the activation epoch for every actual source transition", () => {
    const initial = createInitialDesktopPreferencesState();
    const distributed = desktopPreferencesReducer(initial, {
      type: "preferences-received",
      preferences: {
        ...DEFAULT_DESKTOP_PREFERENCES,
        radarSource: "distributed",
      },
    });
    const main = desktopPreferencesReducer(distributed, {
      type: "preferences-received",
      preferences: { ...DEFAULT_DESKTOP_PREFERENCES },
    });

    expect(distributed.radarActivationEpoch).toBe(1);
    expect(main.preferences.radarSource).toBe("main");
    expect(main.radarActivationEpoch).toBe(2);
  });

  it("keeps the last accepted preferences when hydration fails", () => {
    const ready = desktopPreferencesReducer(
      createInitialDesktopPreferencesState(),
      {
        type: "preferences-received",
        preferences: { ...DEFAULT_DESKTOP_PREFERENCES },
      },
    );
    const failed = desktopPreferencesReducer(ready, {
      type: "preferences-failed",
      message: "native state unavailable",
    });

    expect(failed.preferences).toBe(ready.preferences);
    expect(failed.error).toBe("native state unavailable");
  });
});

describe("useDesktopPreferences initialization", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getDesktopPreferences).mockResolvedValue({
      ...DEFAULT_DESKTOP_PREFERENCES,
    });
    vi.mocked(onDesktopPreferencesUpdated).mockResolvedValue(vi.fn());
    vi.mocked(setDesktopRadarSource).mockResolvedValue({
      ...DEFAULT_DESKTOP_PREFERENCES,
      radarSource: "distributed",
    });
  });

  it("registers first and does not let a late initial read replace a newer event", async () => {
    const listenerRegistration = createDeferred<() => void>();
    const initialRead = createDeferred<DesktopPreferences>();
    const unlisten = vi.fn();
    let emitPreferences!: (preferences: DesktopPreferences) => void;

    vi.mocked(onDesktopPreferencesUpdated).mockImplementation((handler) => {
      emitPreferences = handler;
      return listenerRegistration.promise;
    });
    vi.mocked(getDesktopPreferences).mockReturnValue(initialRead.promise);

    const { result, unmount } = renderHook(() => useDesktopPreferences());

    expect(onDesktopPreferencesUpdated).toHaveBeenCalledOnce();
    expect(getDesktopPreferences).not.toHaveBeenCalled();

    await act(async () => {
      listenerRegistration.resolve(unlisten);
      await listenerRegistration.promise;
    });
    await waitFor(() => expect(getDesktopPreferences).toHaveBeenCalledOnce());

    const updatedPreferences: DesktopPreferences = {
      ...DEFAULT_DESKTOP_PREFERENCES,
      radarSource: "distributed",
    };
    act(() => emitPreferences(updatedPreferences));
    expect(result.current.preferences).toBe(updatedPreferences);

    await act(async () => {
      initialRead.resolve({ ...DEFAULT_DESKTOP_PREFERENCES });
      await initialRead.promise;
    });

    expect(result.current.preferences).toBe(updatedPreferences);
    expect(result.current.hydrated).toBe(true);
    expect(result.current.error).toBeNull();
    unmount();
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("cleans up a listener whose registration completes after unmount", async () => {
    const listenerRegistration = createDeferred<() => void>();
    const unlisten = vi.fn();
    vi.mocked(onDesktopPreferencesUpdated).mockReturnValue(
      listenerRegistration.promise,
    );

    const { unmount } = renderHook(() => useDesktopPreferences());
    expect(getDesktopPreferences).not.toHaveBeenCalled();
    unmount();

    await act(async () => {
      listenerRegistration.resolve(unlisten);
      await listenerRegistration.promise;
    });

    await waitFor(() => expect(unlisten).toHaveBeenCalledOnce());
    expect(getDesktopPreferences).not.toHaveBeenCalled();
  });

  it("projects the complete accepted source command response", async () => {
    const { result } = renderHook(() => useDesktopPreferences());
    await waitFor(() => expect(result.current.hydrated).toBe(true));

    await act(async () => {
      await result.current.setRadarSource("distributed");
    });

    expect(setDesktopRadarSource).toHaveBeenCalledWith("distributed");
    expect(result.current.preferences.radarSource).toBe("distributed");
    expect(result.current.radarActivationEpoch).toBe(1);
  });
});

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T | PromiseLike<T>) => void;
}

function createDeferred<T>(): Deferred<T> {
  let resolve!: Deferred<T>["resolve"];
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}
