import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { sampleSnapshot } from "./test/fixtures";
import { DEFAULT_DESKTOP_PREFERENCES } from "./types/desktop";
import App from "./App";

const mocks = vi.hoisted(() => ({
  getMainExpanded: vi.fn(),
  onMainExpanded: vi.fn(),
  onShowMainDetails: vi.fn(),
  detailsUnlisten: vi.fn(),
  setOption: vi.fn(),
  setOpacity: vi.fn(),
  setRadarSource: vi.fn(),
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
    onShowMainDetails: mocks.onShowMainDetails,
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
    mocks.getMainExpanded.mockResolvedValue(false);
    mocks.onMainExpanded.mockResolvedValue(mocks.unlisten);
    mocks.onShowMainDetails.mockResolvedValue(mocks.detailsUnlisten);
    mocks.setWindowExpanded.mockResolvedValue(undefined);
    mocks.setOption.mockResolvedValue({ ...DEFAULT_DESKTOP_PREFERENCES });
    mocks.setOpacity.mockResolvedValue({ ...DEFAULT_DESKTOP_PREFERENCES });
    mocks.setRadarSource.mockResolvedValue({ ...DEFAULT_DESKTOP_PREFERENCES });
    mocks.useDesktopPreferences.mockReturnValue({
      preferences: { ...DEFAULT_DESKTOP_PREFERENCES },
      radarActivationEpoch: 0,
      hydrated: true,
      error: null,
      setOption: mocks.setOption,
      setOpacity: mocks.setOpacity,
      setRadarSource: mocks.setRadarSource,
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
    expect(mocks.detailsUnlisten).toHaveBeenCalledOnce();
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

  it("opens settings from compact only after expanding and restores compact geometry", async () => {
    render(<App />);
    await waitFor(() => expect(mocks.getMainExpanded).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
    await screen.findByLabelText("Codex Radar 设置");
    expect(mocks.setWindowExpanded).toHaveBeenNthCalledWith(1, true);

    fireEvent.click(screen.getByRole("button", { name: "返回雷达" }));
    await screen.findByLabelText("Codex Model IQ 雷达");
    expect(mocks.setWindowExpanded).toHaveBeenNthCalledWith(2, false);
  });

  it("opens settings from detail and returns without another native resize", async () => {
    mocks.getMainExpanded.mockResolvedValue(true);
    render(<App />);
    await screen.findByLabelText("Codex Model IQ 详情");
    mocks.setWindowExpanded.mockClear();

    fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
    await screen.findByLabelText("Codex Radar 设置");
    fireEvent.click(screen.getByRole("button", { name: "返回雷达" }));

    await screen.findByLabelText("Codex Model IQ 详情");
    expect(mocks.setWindowExpanded).not.toHaveBeenCalled();
  });

  it("honors the native show-details intent while settings is open", async () => {
    let emitShowDetails!: () => void;
    mocks.onShowMainDetails.mockImplementation((handler) => {
      emitShowDetails = handler;
      return Promise.resolve(mocks.detailsUnlisten);
    });
    render(<App />);
    await waitFor(() => expect(mocks.getMainExpanded).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
    await screen.findByLabelText("Codex Radar 设置");
    act(() => emitShowDetails());

    expect(screen.getByLabelText("Codex Model IQ 详情")).toBeTruthy();
    expect(screen.queryByLabelText("Codex Radar 设置")).toBeNull();
  });

  it("registers the show-details intent without waiting for expanded-state registration", async () => {
    const expandedRegistration = createDeferred<() => void>();
    let emitShowDetails!: () => void;
    mocks.onMainExpanded.mockReturnValue(expandedRegistration.promise);
    mocks.onShowMainDetails.mockImplementation((handler) => {
      emitShowDetails = handler;
      return Promise.resolve(mocks.detailsUnlisten);
    });

    const { unmount } = render(<App />);
    await waitFor(() => expect(mocks.onShowMainDetails).toHaveBeenCalledOnce());
    expect(mocks.getMainExpanded).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
    await screen.findByLabelText("Codex Radar 设置");
    act(() => emitShowDetails());

    expect(screen.getByLabelText("Codex Model IQ 详情")).toBeTruthy();
    unmount();
    expect(mocks.detailsUnlisten).toHaveBeenCalledOnce();
    await act(async () => {
      expandedRegistration.resolve(mocks.unlisten);
      await expandedRegistration.promise;
    });
    await waitFor(() => expect(mocks.unlisten).toHaveBeenCalledOnce());
  });

  it("blocks duplicate setting updates and retains the accepted value on failure", async () => {
    const update = createDeferred<typeof DEFAULT_DESKTOP_PREFERENCES>();
    mocks.setOption.mockReturnValue(update.promise);
    render(<App />);
    await waitFor(() => expect(mocks.getMainExpanded).toHaveBeenCalledOnce());
    fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
    await screen.findByLabelText("Codex Radar 设置");

    const launchAtLogin = screen.getByRole("checkbox", { name: /开机自启/ });
    fireEvent.click(launchAtLogin);
    await screen.findByText("正在保存设置");
    fireEvent.click(launchAtLogin);
    expect(mocks.setOption).toHaveBeenCalledOnce();
    expect(mocks.setOption).toHaveBeenCalledWith("launchAtLogin", true);

    await act(async () => {
      update.reject(new Error("native registration failed"));
      await update.promise.catch(() => undefined);
    });

    expect((await screen.findByRole("alert")).textContent).toBe(
      "设置保存失败，请重试",
    );
    expect(
      (screen.getByRole("checkbox", { name: /开机自启/ }) as HTMLInputElement)
        .checked,
    ).toBe(false);
  });

  it("keeps compact mounted when settings expansion fails", async () => {
    mocks.setWindowExpanded.mockRejectedValueOnce(new Error("resize failed"));
    render(<App />);
    await waitFor(() => expect(mocks.getMainExpanded).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
    await screen.findByText("窗口尺寸调整失败，请重试");

    expect(screen.getByLabelText("Codex Model IQ 雷达")).toBeTruthy();
    expect(screen.queryByLabelText("Codex Radar 设置")).toBeNull();
  });

  it("keeps settings mounted when restoring compact geometry fails", async () => {
    render(<App />);
    await waitFor(() => expect(mocks.getMainExpanded).toHaveBeenCalledOnce());
    fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
    await screen.findByLabelText("Codex Radar 设置");
    mocks.setWindowExpanded.mockRejectedValueOnce(new Error("resize failed"));

    fireEvent.click(screen.getByRole("button", { name: "返回雷达" }));

    expect((await screen.findByRole("alert")).textContent).toBe(
      "窗口尺寸调整失败，请重试",
    );
    expect(screen.getByLabelText("Codex Radar 设置")).toBeTruthy();
    expect(screen.queryByLabelText("Codex Model IQ 雷达")).toBeNull();
  });

  it("closes settings through the same compact restore path on Escape", async () => {
    render(<App />);
    await waitFor(() => expect(mocks.getMainExpanded).toHaveBeenCalledOnce());
    fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
    await screen.findByLabelText("Codex Radar 设置");

    fireEvent.keyDown(window, { key: "Escape" });

    await screen.findByLabelText("Codex Model IQ 雷达");
    expect(mocks.setWindowExpanded).toHaveBeenLastCalledWith(false);
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
