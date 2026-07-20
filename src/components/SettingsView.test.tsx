import { fireEvent, render, screen } from "@testing-library/react";
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
