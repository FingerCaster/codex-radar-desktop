import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { DEFAULT_DESKTOP_PREFERENCES } from "../types/desktop";
import { SettingsView } from "./SettingsView";

function renderSettings(
  overrides: Partial<React.ComponentProps<typeof SettingsView>> = {},
) {
  const props: React.ComponentProps<typeof SettingsView> = {
    preferences: { ...DEFAULT_DESKTOP_PREFERENCES },
    pending: null,
    error: null,
    onBack: vi.fn(),
    onSetOption: vi.fn(),
    onSetOpacity: vi.fn(),
    onSetPositionPreset: vi.fn(),
    onSetRadarSource: vi.fn(),
    ...overrides,
  };
  return { ...render(<SettingsView {...props} />), props };
}

describe("SettingsView", () => {
  it("renders accessible source, opacity, boolean, and back controls", () => {
    const { props } = renderSettings();

    expect(screen.getByLabelText("Codex Radar 设置")).toBeTruthy();
    expect(screen.getByRole("button", { name: "返回雷达" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "主站" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
    expect(
      screen.getByRole("button", { name: "分布式" }).getAttribute("aria-pressed"),
    ).toBe("false");
    expect(
      (screen.getByRole("checkbox", { name: /总是置顶/ }) as HTMLInputElement)
        .checked,
    ).toBe(true);
    expect(
      (screen.getByRole("checkbox", { name: /开机自启/ }) as HTMLInputElement)
        .checked,
    ).toBe(false);
    expect(screen.getByRole("button", { name: "100%" }).getAttribute("aria-pressed")).toBe(
      "true",
    );

    fireEvent.click(screen.getByRole("button", { name: "分布式" }));
    expect(props.onSetRadarSource).toHaveBeenCalledWith("distributed");
    fireEvent.click(screen.getByRole("button", { name: "80%" }));
    expect(props.onSetOpacity).toHaveBeenCalledWith(80);
    fireEvent.click(screen.getByRole("checkbox", { name: /开机自启/ }));
    expect(props.onSetOption).toHaveBeenCalledWith("launchAtLogin", true);
    fireEvent.click(screen.getByRole("button", { name: "返回雷达" }));
    expect(props.onBack).toHaveBeenCalledOnce();
  });

  it("emits all five quick-position presets in native-menu order", () => {
    const { props } = renderSettings();
    const group = screen.getByRole("group", { name: "快捷位置" });
    const controls = within(group).getAllByRole("button");
    const actions = [
      ["移到上左", "top-left"],
      ["移到上右", "top-right"],
      ["移到中心", "center"],
      ["移到下左", "bottom-left"],
      ["移到下右", "bottom-right"],
    ] as const;

    expect(controls.map((control) => control.getAttribute("aria-label"))).toEqual(
      actions.map(([label]) => label),
    );
    controls.forEach((control, index) => {
      expect(control.getAttribute("title")).toBe(actions[index][0]);
      fireEvent.click(control);
    });
    actions.forEach(([, preset], index) => {
      expect(props.onSetPositionPreset).toHaveBeenNthCalledWith(index + 1, preset);
    });
  });

  it("keeps explicit position presets enabled while drag locking is active", () => {
    const { props } = renderSettings({
      preferences: {
        ...DEFAULT_DESKTOP_PREFERENCES,
        positionLocked: true,
      },
    });
    const bottomRight = screen.getByRole("button", { name: "移到下右" });

    expect((bottomRight as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(bottomRight);
    expect(props.onSetPositionPreset).toHaveBeenCalledWith("bottom-right");
  });

  it("disables every setting control while a native update is pending", () => {
    renderSettings({ pending: "launchAtLogin" });

    expect(screen.getByRole("status").textContent).toBe("正在保存设置");
    for (const control of screen.getAllByRole("button")) {
      expect((control as HTMLButtonElement).disabled).toBe(true);
    }
    for (const control of screen.getAllByRole("checkbox")) {
      expect((control as HTMLInputElement).disabled).toBe(true);
    }
  });

  it("retains accepted values and exposes a bounded failure status", () => {
    renderSettings({ error: "设置保存失败，请重试" });

    expect(screen.getByRole("alert").textContent).toBe("设置保存失败，请重试");
    expect(
      (screen.getByRole("checkbox", { name: /开机自启/ }) as HTMLInputElement)
        .checked,
    ).toBe(false);
    expect(screen.getByRole("button", { name: "主站" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
  });
});
