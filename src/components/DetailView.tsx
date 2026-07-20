import {
  ExternalLink,
  EyeOff,
  Minimize2,
  Radar,
  RefreshCw,
  Settings,
  Trophy,
  WifiOff,
} from "lucide-react";
import { IconButton } from "./IconButton";
import { ModelMark } from "./ModelMark";
import { getModelDisplayName } from "../lib/model";
import {
  RADAR_STATUS_LABELS,
  type DetailViewProps,
  type ModelScore,
} from "../types/radar";

const scoreFormatter = new Intl.NumberFormat("zh-CN", {
  maximumFractionDigits: 2,
});

const costFormatter = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 3,
});

const dateFormatter = new Intl.DateTimeFormat("zh-CN", {
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});

function finiteNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function formatTaskCount(value: number | null | undefined) {
  return finiteNumber(value) ? String(value) : "--";
}

function formatDuration(entry: ModelScore | null) {
  if (!entry) {
    return "--";
  }

  if (entry.averageTaskTimeHuman) {
    return entry.averageTaskTimeHuman;
  }

  if (!finiteNumber(entry.averageTaskSeconds)) {
    return "--";
  }

  if (entry.averageTaskSeconds < 60) {
    return `${Math.round(entry.averageTaskSeconds)} 秒`;
  }

  const minutes = Math.floor(entry.averageTaskSeconds / 60);
  const seconds = Math.round(entry.averageTaskSeconds % 60);
  return seconds > 0 ? `${minutes}分 ${seconds}秒` : `${minutes} 分钟`;
}

function formatTimestamp(value: string | undefined) {
  if (!value) {
    return "--";
  }

  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "--" : dateFormatter.format(date);
}

function getStatusMessage(
  status: DetailViewProps["status"],
  error: string | null | undefined,
  hasSnapshot: boolean,
) {
  if (status === "stale") {
    return error || "网络不可用，以下为上次成功同步的数据";
  }

  if (status === "unavailable") {
    return error || "暂时无法连接数据源，请手动重试";
  }

  if (status === "refreshing") {
    return "正在向数据源检查最新排行";
  }

  if (!hasSnapshot) {
    return "正在建立首次同步";
  }

  return "排行数据已同步";
}

function getRank(rankings: ModelScore[], index: number) {
  const score = rankings[index]?.score;
  const firstAtScore = rankings.findIndex((entry) => entry.score === score);
  return firstAtScore + 1;
}

export function DetailView({
  snapshot,
  status,
  error,
  positionLocked,
  onRefresh,
  onCollapse,
  onHide,
  onOpenSource,
  onOpenSettings,
}: DetailViewProps) {
  const rankings = snapshot?.rankings ?? [];
  const visibleRankings = rankings.slice(0, 5);
  const leaderIdSet = new Set(snapshot?.leaderIds ?? []);
  const leaders = rankings.filter((entry) => leaderIdSet.has(entry.id));
  const primary = leaders[0] ?? rankings[0] ?? null;
  const leaderCount = leaders.length || (primary ? 1 : 0);
  const leaderName = primary ? getModelDisplayName(primary) : "暂无排行数据";
  const attribution = snapshot?.attribution.text || "数据来源：Codex Radar";
  const statusMessage = getStatusMessage(status, error, snapshot !== null);
  const updatedAt = formatTimestamp(snapshot?.updatedAt);

  return (
    <section
      aria-label="Codex Model IQ 详情"
      aria-busy={status === "booting" || status === "refreshing"}
      className="radar-shell detail-view"
      data-state={status}
    >
      <header
        className="detail-header"
        data-tauri-drag-region={positionLocked ? undefined : true}
      >
        <div
          className="radar-brand"
          data-tauri-drag-region={positionLocked ? undefined : true}
        >
          <Radar aria-hidden="true" size={16} strokeWidth={2} />
          <span data-tauri-drag-region={positionLocked ? undefined : true}>
            Codex Radar
          </span>
          <span
            className={`radar-status radar-status--${status}`}
            role="status"
            title={statusMessage}
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
          <IconButton icon={Settings} label="打开设置" onClick={onOpenSettings} />
          <IconButton icon={Minimize2} label="收起详情" onClick={onCollapse} />
          <IconButton icon={EyeOff} label="隐藏窗口" onClick={onHide} />
        </div>
      </header>

      <main className="detail-content">
        <section className="leader-band" aria-labelledby="leader-title">
          <div className="leader-copy">
            <span className="leader-kicker">
              <Trophy aria-hidden="true" size={14} />
              {leaderCount > 1 ? `${leaderCount} 个模型并列榜首` : "当前 Model IQ 榜首"}
            </span>
            <div className="leader-model-line">
              <ModelMark className="model-mark--leader" model={primary?.model} />
              <h1 id="leader-title" title={leaderName}>
                {leaderName}
              </h1>
            </div>
            <span className="leader-effort" title={primary?.reasoningEffort}>
              {primary?.reasoningEffort || "等待同步"}
            </span>
          </div>
          <div className="leader-score" aria-label={primary ? `IQ ${primary.score}` : "IQ 暂无"}>
            <span>IQ</span>
            <strong>{primary ? scoreFormatter.format(primary.score) : "--"}</strong>
          </div>
          <p className="sync-message" aria-live="polite" title={statusMessage}>
            {statusMessage}
          </p>
        </section>

        <dl className="metric-grid" aria-label="榜首统计">
          <div className="metric-item">
            <dt>通过 / 任务</dt>
            <dd>
              {primary
                ? `${formatTaskCount(primary.passed)} / ${formatTaskCount(primary.tasks)}`
                : "--"}
            </dd>
          </div>
          <div className="metric-item">
            <dt>平均成本</dt>
            <dd>
              {finiteNumber(primary?.averageCostUsd)
                ? costFormatter.format(primary.averageCostUsd)
                : "--"}
            </dd>
          </div>
          <div className="metric-item">
            <dt>平均耗时</dt>
            <dd title={formatDuration(primary)}>{formatDuration(primary)}</dd>
          </div>
          <div className="metric-item">
            <dt>数据更新</dt>
            <dd title={snapshot?.updatedAt}>{updatedAt}</dd>
          </div>
        </dl>

        <section className="detail-rankings" aria-labelledby="ranking-title">
          <div className="ranking-heading">
            <h2 id="ranking-title">当前排行</h2>
            <span>前 5 名</span>
          </div>

          {visibleRankings.length > 0 ? (
            <ol className="ranking-list">
              {visibleRankings.map((entry, index) => {
                const isLeader = leaderIdSet.has(entry.id) || index === 0;
                const displayName = getModelDisplayName(entry);

                return (
                  <li className={isLeader ? "ranking-row is-leader" : "ranking-row"} key={entry.id}>
                    <span className="ranking-position" aria-label={`第 ${getRank(rankings, index)} 名`}>
                      {getRank(rankings, index)}
                    </span>
                    <span className="ranking-model-cell">
                      <ModelMark className="model-mark--ranking" model={entry.model} />
                      <span className="ranking-model">
                        <strong title={displayName}>{displayName}</strong>
                        <span title={entry.reasoningEffort}>{entry.reasoningEffort}</span>
                      </span>
                    </span>
                    <span
                      className="ranking-pass"
                      title={`${formatTaskCount(entry.passed)} / ${formatTaskCount(entry.tasks)} 个任务通过`}
                    >
                      {formatTaskCount(entry.passed)}/{formatTaskCount(entry.tasks)}
                    </span>
                    <strong className="ranking-score" aria-label={`IQ ${entry.score}`}>
                      {scoreFormatter.format(entry.score)}
                    </strong>
                  </li>
                );
              })}
            </ol>
          ) : (
            <div className="ranking-empty" role="status">
              <WifiOff aria-hidden="true" size={20} />
              <span>暂时无法获取排行</span>
            </div>
          )}
        </section>

        <footer className="source-footer">
          <p title={attribution}>{attribution}</p>
          <button
            aria-label="在浏览器中查看 Codex Radar 数据来源"
            className="source-link"
            onClick={() => onOpenSource()}
            title="查看 Codex Radar 数据来源"
            type="button"
          >
            <ExternalLink aria-hidden="true" size={14} strokeWidth={2} />
            查看来源
          </button>
        </footer>
      </main>
    </section>
  );
}

export default DetailView;
