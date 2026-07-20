import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { sampleSnapshot } from "./test/fixtures";
import { DEFAULT_DESKTOP_PREFERENCES } from "./types/desktop";
import App from "./App";

const mocks = vi.hoisted(() => ({
  getMainExpanded: vi.fn(),
  onMainExpanded: vi.fn(),
  setWindowExpanded: vi.fn(async () => undefined),
  unlisten: vi.fn(),
  updateCompanionProjection: vi.fn(async () => undefined),
  useDesktopPreferences: vi.fn(),
  useRadar: vi.fn(),
}));

vi.mock("./hooks/useDesktopPreferences", () => ({
  useDesktopPreferences: mocks.useDesktopPreferences,
}));

vi.mock("./hooks/useRadar", () => ({
  useRadar: mocks.useRadar,
}));

vi.mock("./lib/desktop", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/desktop")>();
  return {
    ...actual,
    getCurrentWebviewLabel: () => "main",
    getMainExpanded: mocks.getMainExpanded,
    onMainExpanded: mocks.onMainExpanded,
    updateCompanionProjection: mocks.updateCompanionProjection,
  };
});

vi.mock("./lib/radar", () => ({
  hideWindow: vi.fn(async () => undefined),
  openSourceSite: vi.fn(async () => undefined),
  setWindowExpanded: mocks.setWindowExpanded,
}));

describe("App main expanded-state hydration", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.useDesktopPreferences.mockReturnValue({
      preferences: { ...DEFAULT_DESKTOP_PREFERENCES },
      radarActivationEpoch: 0,
      hydrated: true,
      error: null,
      setOption: vi.fn(),
      setOpacity: vi.fn(),
    });
    mocks.useRadar.mockReturnValue({
      source: "main",
      activationEpoch: 0,
      snapshot: sampleSnapshot,
      status: "stale",
      error: null,
      notificationsEnabled: true,
      refresh: vi.fn(async () => undefined),
    });
  });

  it("does not let a late initial value override a newer expanded event", async () => {
    const initialExpanded = createDeferred<boolean>();
    let emitExpanded!: (expanded: boolean) => void;
    mocks.onMainExpanded.mockImplementation((handler) => {
      emitExpanded = handler;
      return Promise.resolve(mocks.unlisten);
    });
    mocks.getMainExpanded.mockReturnValue(initialExpanded.promise);

    const { unmount } = render(<App />);
    await waitFor(() => expect(mocks.getMainExpanded).toHaveBeenCalledOnce());

    act(() => emitExpanded(true));
    expect(screen.getByLabelText("Codex Model IQ 详情")).toBeTruthy();

    await act(async () => {
      initialExpanded.resolve(false);
      await initialExpanded.promise;
    });

    expect(screen.getByLabelText("Codex Model IQ 详情")).toBeTruthy();
    unmount();
    expect(mocks.unlisten).toHaveBeenCalledOnce();
  });

  it("ignores a late initial error after an expanded event", async () => {
    const initialExpanded = createDeferred<boolean>();
    let emitExpanded!: (expanded: boolean) => void;
    mocks.onMainExpanded.mockImplementation((handler) => {
      emitExpanded = handler;
      return Promise.resolve(mocks.unlisten);
    });
    mocks.getMainExpanded.mockReturnValue(initialExpanded.promise);

    const { unmount } = render(<App />);
    await waitFor(() => expect(mocks.getMainExpanded).toHaveBeenCalledOnce());

    act(() => emitExpanded(true));
    await act(async () => {
      initialExpanded.reject(new Error("late read failed"));
      await initialExpanded.promise.catch(() => undefined);
    });

    expect(screen.getByLabelText("Codex Model IQ 详情")).toBeTruthy();
    expect(screen.queryByText("窗口状态同步失败，请重试")).toBeNull();
    unmount();
  });
});

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
