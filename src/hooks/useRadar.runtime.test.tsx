import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  ensureNotificationPermission,
  getRadarSnapshot,
  loadCachedSnapshot,
  onRefreshFailed,
  onRefreshRequested,
  onSnapshotUpdated,
  refreshRadar,
  type RefreshOutcome,
} from "../lib/radar";
import { sampleSnapshot } from "../test/fixtures";
import type { RadarSource } from "../types/radar";
import { useRadar } from "./useRadar";

vi.mock("../lib/radar", () => ({
  ensureNotificationPermission: vi.fn(async () => true),
  getRadarSnapshot: vi.fn(async () => null),
  isSnapshotOlder: vi.fn(() => false),
  loadCachedSnapshot: vi.fn(() => null),
  normalizeRefreshFailure: vi.fn((_error: unknown, source: RadarSource) => ({
    source,
    kind: "network",
    message: "offline",
    occurredAt: "2026-07-19T16:00:00.000Z",
  })),
  onRefreshFailed: vi.fn(async () => vi.fn()),
  onRefreshRequested: vi.fn(async () => vi.fn()),
  onSnapshotUpdated: vi.fn(async () => vi.fn()),
  refreshRadar: vi.fn(async () => {
    throw new Error("offline");
  }),
  saveCachedSnapshot: vi.fn(),
}));

describe("useRadar runtime modes", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(loadCachedSnapshot).mockReturnValue(null);
    vi.mocked(getRadarSnapshot).mockResolvedValue(null);
  });

  it("does not read the default main cache or start side effects before preferences hydrate", () => {
    const { result, unmount } = renderHook(() =>
      useRadar({ source: "main", enabled: false }),
    );

    expect(result.current.snapshot).toBeNull();
    expect(result.current.status).toBe("booting");
    expect(loadCachedSnapshot).not.toHaveBeenCalled();
    expect(getRadarSnapshot).not.toHaveBeenCalled();
    expect(onSnapshotUpdated).not.toHaveBeenCalled();
    expect(refreshRadar).not.toHaveBeenCalled();
    expect(ensureNotificationPermission).not.toHaveBeenCalled();
    unmount();
  });

  it("keeps a passive taskbar renderer read-only for its hydrated source", async () => {
    const addEventListener = vi.spyOn(window, "addEventListener");
    const { unmount } = renderHook(() =>
      useRadar({ passive: true, source: "distributed", enabled: true }),
    );

    await waitFor(() => expect(getRadarSnapshot).toHaveBeenCalledOnce());
    expect(loadCachedSnapshot).toHaveBeenCalledWith("distributed");
    expect(onSnapshotUpdated).toHaveBeenCalledOnce();
    expect(onRefreshFailed).toHaveBeenCalledOnce();
    expect(onRefreshRequested).not.toHaveBeenCalled();
    expect(refreshRadar).not.toHaveBeenCalled();
    expect(ensureNotificationPermission).not.toHaveBeenCalled();
    expect(addEventListener).not.toHaveBeenCalledWith(
      "online",
      expect.any(Function),
    );

    unmount();
    addEventListener.mockRestore();
  });

  it("retains active refresh and recovery sources in the main renderer", async () => {
    const addEventListener = vi.spyOn(window, "addEventListener");
    const { unmount } = renderHook(() =>
      useRadar({ source: "main", enabled: true }),
    );

    await waitFor(() => expect(refreshRadar).toHaveBeenCalledOnce());
    expect(onRefreshRequested).toHaveBeenCalledOnce();
    expect(ensureNotificationPermission).toHaveBeenCalledOnce();
    expect(addEventListener).toHaveBeenCalledWith(
      "online",
      expect.any(Function),
    );

    unmount();
    addEventListener.mockRestore();
  });

  it("synchronously hides the old source before the selection effect runs", () => {
    vi.mocked(loadCachedSnapshot).mockImplementation((source) =>
      source === "main" ? sampleSnapshot : null,
    );
    const { result, rerender, unmount } = renderHook(
      ({ source }: { source: RadarSource }) =>
        useRadar({ passive: true, source, enabled: true }),
      { initialProps: { source: "main" } },
    );

    expect(result.current.snapshot).toBe(sampleSnapshot);
    rerender({ source: "distributed" });

    expect(result.current.source).toBe("distributed");
    expect(result.current.snapshot).toBeNull();
    expect(result.current.status).toBe("booting");
    expect(onSnapshotUpdated).toHaveBeenCalledOnce();
    expect(onRefreshFailed).toHaveBeenCalledOnce();
    expect(refreshRadar).not.toHaveBeenCalled();
    unmount();
  });

  it("ignores a successful command result from an earlier A-B-A activation", async () => {
    const pending = createDeferred<RefreshOutcome>();
    const reactivatedSnapshot = {
      ...sampleSnapshot,
      checkedAt: "2026-07-20T04:00:00Z",
    };
    const oldActivationSnapshot = {
      ...sampleSnapshot,
      updatedAt: "2026-07-20T05:00:00Z",
      checkedAt: "2026-07-20T05:00:01Z",
    };
    vi.mocked(refreshRadar).mockReturnValueOnce(pending.promise);
    vi.mocked(loadCachedSnapshot).mockImplementation((source) =>
      source === "main" ? reactivatedSnapshot : null,
    );

    const { result, rerender, unmount } = renderHook(
      ({ source, activationEpoch }: RadarActivationProps) =>
        useRadar({
          passive: true,
          source,
          activationEpoch,
          enabled: true,
        }),
      { initialProps: { source: "main", activationEpoch: 0 } },
    );

    let refreshPromise!: Promise<void>;
    act(() => {
      refreshPromise = result.current.refresh();
    });
    await waitFor(() => expect(result.current.status).toBe("refreshing"));

    // React may batch both preference actions and render only the final A2.
    rerender({ source: "main", activationEpoch: 2 });
    await waitFor(() => expect(result.current.activationEpoch).toBe(2));

    await act(async () => {
      pending.resolve({
        snapshot: oldActivationSnapshot,
        notModified: false,
        leaderChanged: false,
      });
      await refreshPromise;
    });

    expect(result.current.snapshot).toBe(reactivatedSnapshot);
    expect(result.current.status).toBe("stale");
    expect(result.current.error).toBeNull();
    unmount();
  });

  it("ignores a failed command result from an earlier A-B-A activation", async () => {
    const pending = createDeferred<RefreshOutcome>();
    const reactivatedSnapshot = {
      ...sampleSnapshot,
      checkedAt: "2026-07-20T04:00:00Z",
    };
    vi.mocked(refreshRadar).mockReturnValueOnce(pending.promise);
    vi.mocked(loadCachedSnapshot).mockImplementation((source) =>
      source === "main" ? reactivatedSnapshot : null,
    );

    const { result, rerender, unmount } = renderHook(
      ({ source, activationEpoch }: RadarActivationProps) =>
        useRadar({
          passive: true,
          source,
          activationEpoch,
          enabled: true,
        }),
      { initialProps: { source: "main", activationEpoch: 0 } },
    );

    let refreshPromise!: Promise<void>;
    act(() => {
      refreshPromise = result.current.refresh();
    });
    await waitFor(() => expect(result.current.status).toBe("refreshing"));

    rerender({ source: "distributed", activationEpoch: 1 });
    rerender({ source: "main", activationEpoch: 2 });
    await waitFor(() => expect(result.current.activationEpoch).toBe(2));

    await act(async () => {
      pending.reject(new Error("late A0 failure"));
      await refreshPromise;
    });

    expect(result.current.snapshot).toBe(reactivatedSnapshot);
    expect(result.current.status).toBe("stale");
    expect(result.current.error).toBeNull();
    unmount();
  });
});

interface RadarActivationProps {
  source: RadarSource;
  activationEpoch: number;
}

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T | PromiseLike<T>) => void;
  reject: (reason?: unknown) => void;
}

function createDeferred<T>(): Deferred<T> {
  let resolve!: Deferred<T>["resolve"];
  let reject!: Deferred<T>["reject"];
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}
