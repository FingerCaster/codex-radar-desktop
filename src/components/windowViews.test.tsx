import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { sampleSnapshot } from "../test/fixtures";
import { CompactView } from "./CompactView";
import { DetailView } from "./DetailView";

const sharedProps = {
  snapshot: sampleSnapshot,
  status: "ready" as const,
  error: null,
  onRefresh: vi.fn(),
  onHide: vi.fn(),
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
});
