export type RadarStatus =
  | "booting"
  | "ready"
  | "refreshing"
  | "stale"
  | "unavailable";

export type RadarSource = "main" | "distributed";

export interface ModelScore {
  id: string;
  label: string;
  model: string;
  reasoningEffort: string;
  score: number;
  status: string | null;
  passed: number | null;
  tasks: number | null;
  validTasks: number | null;
  averageCostUsd: number | null;
  averageTaskSeconds: number | null;
  averageTaskTimeHuman: string | null;
  wallTimeHuman: string | null;
}

export interface RadarAttribution {
  text: string;
  url: string;
}

export interface RadarSnapshot {
  schemaVersion: string;
  source: RadarSource;
  updatedAt: string;
  checkedAt: string;
  leaderIds: string[];
  rankings: ModelScore[];
  attribution: RadarAttribution;
  sourceUrl: string;
}

export type RadarAction = () => void | Promise<void>;
export type OpenSourceAction = RadarAction;

export interface RadarViewProps {
  snapshot: RadarSnapshot | null;
  status: RadarStatus;
  error?: string | null;
  positionLocked: boolean;
  onRefresh: RadarAction;
  onHide: RadarAction;
}

export interface CompactViewProps extends RadarViewProps {
  onExpand: RadarAction;
}

export interface DetailViewProps extends RadarViewProps {
  onCollapse: RadarAction;
  onOpenSource: OpenSourceAction;
}

export const RADAR_STATUS_LABELS: Record<RadarStatus, string> = {
  booting: "连接中",
  ready: "已同步",
  refreshing: "刷新中",
  stale: "离线 / 旧数据",
  unavailable: "离线",
};
