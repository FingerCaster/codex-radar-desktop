import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import codexLogo from "../assets/codex-logo.svg";
import lunaLogo from "../assets/luna-transparent.png";
import solLogo from "../assets/sol-transparent.png";
import terraLogo from "../assets/terra-transparent.png";
import { sampleSnapshot } from "../test/fixtures";
import { CompactView } from "./CompactView";
import { DetailView } from "./DetailView";

const sharedProps = {
  snapshot: sampleSnapshot,
  status: "ready" as const,
  error: null,
  onRefresh: vi.fn(),
  onHide: vi.fn(),
  onOpenSettings: vi.fn(),
};

describe("window drag regions", () => {
  it("removes and restores every compact drag marker from native state", () => {
    const { container, rerender } = render(
      <CompactView
        {...sharedProps}
        onExpand={vi.fn()}
        positionLocked={false}
      />,
    );
    expect(container.querySelectorAll("[data-tauri-drag-region]")).toHaveLength(
      3,
    );

    rerender(
      <CompactView
        {...sharedProps}
        onExpand={vi.fn()}
        positionLocked
      />,
    );
    expect(container.querySelectorAll("[data-tauri-drag-region]")).toHaveLength(
      0,
    );
  });

  it("removes and restores every detail drag marker from native state", () => {
    const { container, rerender } = render(
      <DetailView
        {...sharedProps}
        onCollapse={vi.fn()}
        onOpenSource={vi.fn()}
        positionLocked={false}
      />,
    );
    expect(container.querySelectorAll("[data-tauri-drag-region]")).toHaveLength(
      3,
    );

    rerender(
      <DetailView
        {...sharedProps}
        onCollapse={vi.fn()}
        onOpenSource={vi.fn()}
        positionLocked
      />,
    );
    expect(container.querySelectorAll("[data-tauri-drag-region]")).toHaveLength(
      0,
    );
  });

  it("requests the selected source through a zero-argument semantic action", () => {
    const onOpenSource = vi.fn();
    render(
      <DetailView
        {...sharedProps}
        onCollapse={vi.fn()}
        onOpenSource={onOpenSource}
        positionLocked={false}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", {
        name: "在浏览器中查看 Codex Radar 数据来源",
      }),
    );
    expect(onOpenSource).toHaveBeenCalledOnce();
    expect(onOpenSource).toHaveBeenCalledWith();
  });

  it("exposes accessible settings actions in compact and detail headers", () => {
    const onOpenSettings = vi.fn();
    const { rerender } = render(
      <CompactView
        {...sharedProps}
        onExpand={vi.fn()}
        onOpenSettings={onOpenSettings}
        positionLocked={false}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
    expect(onOpenSettings).toHaveBeenCalledOnce();

    rerender(
      <DetailView
        {...sharedProps}
        onCollapse={vi.fn()}
        onOpenSettings={onOpenSettings}
        onOpenSource={vi.fn()}
        positionLocked={false}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "打开设置" }));
    expect(onOpenSettings).toHaveBeenCalledTimes(2);
  });
});

describe("model marks", () => {
  it("uses the compact leader identifier while keeping its mark decorative", () => {
    const { container } = render(
      <CompactView
        {...sharedProps}
        onExpand={vi.fn()}
        positionLocked={false}
      />,
    );

    const marks = container.querySelectorAll("img.model-mark");
    expect(marks).toHaveLength(1);
    expect(marks[0]?.getAttribute("alt")).toBe("");
    expect(marks[0]?.getAttribute("aria-hidden")).toBe("true");
    expect(marks[0]?.getAttribute("src")).toBe(solLogo);
    expect(screen.queryByRole("img")).toBeNull();
    expect(
      screen.getByRole("button", {
        name: "打开 GPT-5.6 Sol 的排行详情",
      }),
    ).toBeTruthy();
  });

  it("resolves the detail leader and every ranking row from entry.model", () => {
    const rankedSnapshot = {
      ...sampleSnapshot,
      rankings: [
        sampleSnapshot.rankings[0],
        {
          ...sampleSnapshot.rankings[0],
          id: "gpt-5.6-terra:high",
          label: "GPT-5.6 Terra high",
          model: "gpt-5.6-terra",
          reasoningEffort: "high",
          score: 104.2,
        },
        {
          ...sampleSnapshot.rankings[0],
          id: "gpt-5.6-luna:medium",
          label: "GPT-5.6 Luna medium",
          model: "gpt-5.6-luna",
          reasoningEffort: "medium",
          score: 101.8,
        },
        {
          ...sampleSnapshot.rankings[0],
          id: "gpt-5.5-codex-max:high",
          label: "GPT-5.5 Codex Max high",
          model: "gpt-5.5-codex-max",
          reasoningEffort: "high",
          score: 98.4,
        },
      ],
    };
    const { container } = render(
      <DetailView
        {...sharedProps}
        onCollapse={vi.fn()}
        onOpenSource={vi.fn()}
        positionLocked={false}
        snapshot={rankedSnapshot}
      />,
    );

    const leaderMark = container.querySelector("img.model-mark--leader");
    const rankingMarks = container.querySelectorAll("img.model-mark--ranking");
    const marks = container.querySelectorAll("img.model-mark");
    expect(leaderMark?.getAttribute("src")).toBe(solLogo);
    expect(
      Array.from(rankingMarks, (mark) => mark.getAttribute("src")),
    ).toEqual([solLogo, terraLogo, lunaLogo, codexLogo]);
    expect(marks).toHaveLength(5);
    expect(
      Array.from(marks).every(
        (mark) =>
          mark.getAttribute("alt") === "" &&
          mark.getAttribute("aria-hidden") === "true",
      ),
    ).toBe(true);
    expect(screen.queryByRole("img")).toBeNull();
    expect(
      screen.getByRole("heading", { level: 1, name: "GPT-5.6 Sol" }),
    ).toBeTruthy();
  });
});
