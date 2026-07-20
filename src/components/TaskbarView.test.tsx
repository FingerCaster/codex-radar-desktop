import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import solLogo from "../assets/sol-transparent.png";
import type { CompanionProjection } from "../types/desktop";
import { TaskbarView } from "./TaskbarView";

const projection: CompanionProjection = {
  modelName: "GPT-5.6 Sol",
  reasoningEffort: "max",
  scoreText: "106.3",
  tieCount: 0,
  statusLabel: "已同步",
};

describe("TaskbarView", () => {
  it("renders the fixed two-row projection and opens details", () => {
    const onShowDetails = vi.fn();
    const { container } = render(
      <TaskbarView
        onOpenContextMenu={vi.fn()}
        onShowDetails={onShowDetails}
        projection={projection}
        status="ready"
      />,
    );

    expect(screen.getByText("GPT-5.6 Sol")).toBeTruthy();
    expect(screen.getByText("max")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe("已同步");
    expect(screen.getByText("106.3")).toBeTruthy();
    expect(container.querySelector(".taskbar-primary-row")).toBeTruthy();
    expect(container.querySelector(".taskbar-score-row")).toBeTruthy();
    const mark = container.querySelector("img.model-mark--taskbar");
    expect(mark?.getAttribute("alt")).toBe("");
    expect(mark?.getAttribute("aria-hidden")).toBe("true");
    expect(mark?.getAttribute("src")).toBe(solLogo);
    expect(screen.queryByRole("img")).toBeNull();
    const button = screen.getByRole("button", {
      name: /GPT-5\.6 Sol.*effort max.*同步状态 已同步.*IQ 106\.3/,
    });
    const liveStatus = screen.getByRole("status");
    expect(button.contains(liveStatus)).toBe(false);
    expect(liveStatus.getAttribute("aria-live")).toBe("polite");
    expect(
      container.querySelector(".taskbar-status")?.getAttribute("aria-hidden"),
    ).toBe("true");
    fireEvent.click(button);
    expect(onShowDetails).toHaveBeenCalledOnce();
  });

  it("keeps stale status and a bounded tie marker in the fixed layout", () => {
    render(
      <TaskbarView
        onOpenContextMenu={vi.fn()}
        onShowDetails={vi.fn()}
        projection={{ ...projection, tieCount: 2, statusLabel: "离线 / 旧数据" }}
        status="stale"
      />,
    );

    expect(screen.getByText("+2")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe("离线 / 旧数据");
    expect(
      screen.getByRole("button", {
        name: /effort max.*同步状态 离线 \/ 旧数据.*IQ 106\.3，3 个模型并列榜首/,
      }),
    ).toBeTruthy();
  });

  it("renders a busy, bounded empty projection without fabricating data", () => {
    render(
      <TaskbarView
        onOpenContextMenu={vi.fn()}
        onShowDetails={vi.fn()}
        projection={{
          modelName: "",
          reasoningEffort: "",
          scoreText: "--",
          tieCount: 0,
          statusLabel: "连接中",
        }}
        status="booting"
      />,
    );

    expect(screen.getByText("暂无数据")).toBeTruthy();
    expect(screen.getAllByText("--")).toHaveLength(2);
    expect(screen.getByRole("status").textContent).toBe("连接中");
    expect(
      screen.getByLabelText("Codex Model IQ 任务栏伴随窗口").getAttribute(
        "aria-busy",
      ),
    ).toBe("true");
    expect(
      screen.getByRole("button", {
        name: /暂无数据.*effort --.*同步状态 连接中.*IQ --/,
      }),
    ).toBeTruthy();
  });

  it("opens the native shared menu on context-menu input", () => {
    const onOpenContextMenu = vi.fn();
    const onShowDetails = vi.fn();
    render(
      <TaskbarView
        onOpenContextMenu={onOpenContextMenu}
        onShowDetails={onShowDetails}
        projection={projection}
        status="ready"
      />,
    );

    fireEvent.contextMenu(screen.getByRole("button"));
    expect(onOpenContextMenu).toHaveBeenCalledOnce();
    expect(onShowDetails).not.toHaveBeenCalled();
  });
});
