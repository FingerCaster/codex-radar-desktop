import type { MouseEvent } from "react";

import type { DesktopAction, TaskbarViewProps } from "../types/desktop";

function runAction(action: DesktopAction) {
  try {
    void Promise.resolve(action()).catch(() => undefined);
  } catch {
    // Native command failures leave the tray and main window as recovery paths.
  }
}

export function TaskbarView({
  projection,
  status,
  onShowDetails,
  onOpenContextMenu,
}: TaskbarViewProps) {
  const modelName = projection.modelName || "暂无数据";
  const effort = projection.reasoningEffort || "--";
  const statusLabel = projection.statusLabel || "等待同步";
  const tieAnnouncement =
    projection.tieCount > 0
      ? `，${projection.tieCount + 1} 个模型并列榜首`
      : "";

  const handleContextMenu = (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    runAction(onOpenContextMenu);
  };

  return (
    <section
      aria-label="Codex Model IQ 任务栏伴随窗口"
      aria-busy={status === "booting" || status === "refreshing"}
      className="taskbar-view"
      data-state={status}
    >
      <button
        aria-label={`打开 ${modelName} 的完整排行，effort ${effort}，同步状态 ${statusLabel}，IQ ${projection.scoreText}${tieAnnouncement}`}
        className="taskbar-surface"
        onClick={() => runAction(onShowDetails)}
        onContextMenu={handleContextMenu}
        title={`${modelName} · ${effort} · ${statusLabel} · IQ ${projection.scoreText}`}
        type="button"
      >
        <span className="taskbar-primary-row">
          <strong className="taskbar-model" title={modelName}>
            {modelName}
          </strong>
          <span className="taskbar-effort" title={`effort ${effort}`}>
            {effort}
          </span>
          <span
            aria-hidden="true"
            className="taskbar-status"
            title={statusLabel}
          >
            {statusLabel}
          </span>
        </span>

        <span
          aria-label={`IQ ${projection.scoreText}`}
          className="taskbar-score-row"
        >
          <span className="taskbar-score-label">IQ</span>
          <strong className="taskbar-score-value">
            {projection.scoreText}
          </strong>
          {projection.tieCount > 0 && (
            <span
              className="taskbar-tie"
              title={`${projection.tieCount + 1} 个模型并列榜首`}
            >
              +{projection.tieCount}
            </span>
          )}
        </span>
      </button>
      <span aria-live="polite" className="sr-only" role="status">
        {statusLabel}
      </span>
    </section>
  );
}

export default TaskbarView;
