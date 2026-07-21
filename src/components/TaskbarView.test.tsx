import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import "../App.css";
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

  it("keeps long metadata inside the fixed surface tracks", () => {
    const longProjection: CompanionProjection = {
      modelName: "GPT-5.6 Sol enterprise reasoning preview build",
      reasoningEffort: "maximum-reasoning",
      scoreText: "123456789.123",
      tieCount: 999,
      statusLabel: "离线 / 旧数据 / 等待重试",
    };
    const { container } = render(
      <TaskbarView
        onOpenContextMenu={vi.fn()}
        onShowDetails={vi.fn()}
        projection={longProjection}
        status="stale"
      />,
    );

    const view = screen.getByLabelText("Codex Model IQ 任务栏伴随窗口");
    const surface = screen.getByRole("button");
    const primary = container.querySelector<HTMLElement>(
      ".taskbar-primary-row",
    );
    const scoreRow = container.querySelector<HTMLElement>(
      ".taskbar-score-row",
    );

    expect(getComputedStyle(view).width).toBe("168px");
    expect(getComputedStyle(view).maxWidth).toBe("100%");
    expect(getComputedStyle(view).height).toBe("30px");
    expect(getComputedStyle(view).maxHeight).toBe("100%");
    expect(getComputedStyle(surface).padding).toBe("2px 3px");
    expect(getComputedStyle(primary as HTMLElement).gridTemplateColumns).toBe(
      "10px minmax(0, 1fr) 34px 30px",
    );
    expect(getComputedStyle(scoreRow as HTMLElement).gridTemplateColumns).toBe(
      "17px minmax(0, 1fr) minmax(0, max-content)",
    );
    expect(surface.getAttribute("title")).toBe(
      `${longProjection.modelName} · ${longProjection.reasoningEffort} · ${longProjection.statusLabel} · IQ ${longProjection.scoreText}`,
    );
    expect(
      container.querySelector(".taskbar-model")?.getAttribute("title"),
    ).toBe(longProjection.modelName);
    expect(
      container.querySelector(".taskbar-status")?.getAttribute("title"),
    ).toBe(longProjection.statusLabel);
    expect(
      container.querySelector(".taskbar-tie")?.getAttribute("title"),
    ).toBe(
      "1000 个模型并列榜首",
    );
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
