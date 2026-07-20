import type { RadarSnapshot } from "../types/radar";

export const sampleSnapshot: RadarSnapshot = {
  schemaVersion: "2.0",
  source: "main",
  updatedAt: "2026-07-19T21:56:42+08:00",
  checkedAt: "2026-07-19T14:00:00Z",
  leaderIds: ["gpt-5.6-sol:max"],
  rankings: [
    {
      id: "gpt-5.6-sol:max",
      label: "GPT-5.6 Sol max",
      model: "gpt-5.6-sol",
      reasoningEffort: "max",
      score: 106.3,
      status: "green",
      passed: 79,
      tasks: 112,
      validTasks: 112,
      averageCostUsd: 10.276539,
      averageTaskSeconds: 2383.018,
      averageTaskTimeHuman: "40分钟",
      wallTimeHuman: "74小时8分",
    },
  ],
  attribution: {
    text: "数据来自 Codex 雷达 codexradar.com",
    url: "https://codexradar.com",
  },
  sourceUrl: "https://codex-reset-radar.pages.dev/current.json",
};
