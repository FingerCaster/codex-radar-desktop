import { ChevronRight, EyeOff, Maximize2, Radar, RefreshCw } from "lucide-react";
import { IconButton } from "./IconButton";
import { getModelDisplayName } from "../lib/model";
import {
  RADAR_STATUS_LABELS,
  type CompactViewProps,
  type ModelScore,
} from "../types/radar";

const scoreFormatter = new Intl.NumberFormat("zh-CN", {
  maximumFractionDigits: 2,
});

function findLeaders(
  rankings: ModelScore[],
  leaderIds: string[],
): ModelScore[] {
  const leaders = new Set(leaderIds);
  const matches = rankings.filter((entry) => leaders.has(entry.id));

  if (matches.length > 0) {
    return matches;
  }

  return rankings.slice(0, 1);
}

function getCompactMessage(
  status: CompactViewProps["status"],
  error: string | null | undefined,
  hasSnapshot: boolean,
) {
  if (status === "stale") {
    return error || "连接失败，正在显示上次数据";
  }

  if (status === "unavailable") {
    return error || "暂时无法获取排行";
  }

  if (status === "refreshing") {
    return "正在检查最新排行";
  }

  if (!hasSnapshot) {
    return "正在获取模型排行";
  }

  return "当前 Model IQ 榜首";
}

export function CompactView({
  snapshot,
  status,
  error,
  positionLocked,
  onRefresh,
  onExpand,
  onHide,
}: CompactViewProps) {
  const rankings = snapshot?.rankings ?? [];
  const leaders = findLeaders(rankings, snapshot?.leaderIds ?? []);
  const primary = leaders[0] ?? null;
  const extraLeaderCount = Math.max(0, leaders.length - 1);
  const leaderName = primary ? getModelDisplayName(primary) : "暂无数据";
  const reasoningEffort = primary?.reasoningEffort || "等待同步";
  const message = getCompactMessage(status, error, snapshot !== null);

  return (
    <section
      aria-label="Codex Model IQ 雷达"
      aria-busy={status === "booting" || status === "refreshing"}
      className="radar-shell compact-view"
      data-state={status}
    >
      <header
        className="compact-header"
        data-tauri-drag-region={positionLocked ? undefined : true}
      >
        <div
          className="radar-brand"
          data-tauri-drag-region={positionLocked ? undefined : true}
        >
          <Radar aria-hidden="true" size={15} strokeWidth={2} />
          <span data-tauri-drag-region={positionLocked ? undefined : true}>
            Codex Radar
          </span>
          <span
            aria-live="polite"
            className={`radar-status radar-status--${status}`}
            role="status"
            title={message}
          >
            {RADAR_STATUS_LABELS[status]}
          </span>
        </div>

        <div className="window-controls">
          <IconButton
            className={status === "refreshing" ? "is-spinning" : undefined}
            disabled={status === "refreshing"}
            icon={RefreshCw}
            label={status === "refreshing" ? "正在刷新" : "刷新数据"}
            onClick={onRefresh}
          />
          <IconButton icon={EyeOff} label="隐藏窗口" onClick={onHide} />
          <IconButton icon={Maximize2} label="展开详情" onClick={onExpand} />
        </div>
      </header>

      <button
        aria-label={`打开 ${leaderName} 的排行详情`}
        className="compact-summary"
        onClick={onExpand}
        type="button"
      >
        <span className="compact-copy">
          <span className="compact-kicker" title={message}>
            {message}
          </span>
          <span className="compact-model-line">
            <strong className="compact-model" title={leaderName}>
              {leaderName}
            </strong>
            {extraLeaderCount > 0 && (
              <span className="tie-count" title={`${leaders.length} 个模型并列榜首`}>
                +{extraLeaderCount}
              </span>
            )}
          </span>
          <span className="compact-effort" title={reasoningEffort}>
            {reasoningEffort}
          </span>
        </span>

        <span className="compact-score" aria-label={primary ? `IQ ${primary.score}` : "IQ 暂无"}>
          <span>IQ</span>
          <strong>{primary ? scoreFormatter.format(primary.score) : "--"}</strong>
        </span>
        <ChevronRight aria-hidden="true" className="compact-chevron" size={17} />
      </button>
    </section>
  );
}

export default CompactView;
