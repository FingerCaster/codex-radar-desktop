use std::{cmp::Ordering, collections::BTreeMap};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use super::domain::{Attribution, ModelScore, RadarSnapshot, RadarSource};

pub const PUBLIC_SUMMARY_URL: &str = "https://codex-reset-radar.pages.dev/current.json";
pub const ATTRIBUTION_URL: &str = "https://codexradar.com";
pub const DISTRIBUTED_TABLE_URL: &str = "https://api.codexradar.com/api/v1/table";
pub const DISTRIBUTED_ATTRIBUTION_URL: &str = "https://deng.codexradar.com";

const SUPPORTED_SCHEMA_VERSION: &str = "2.0";
const PUBLIC_SUMMARY_TYPE: &str = "public_summary";
const DEFAULT_ATTRIBUTION_TEXT: &str = "数据来自 Codex 雷达 codexradar.com";
const DISTRIBUTED_SCHEMA_VERSION: u64 = 1;
const DISTRIBUTED_ATTRIBUTION_TEXT: &str = "数据来自分布式 Codex 雷达 deng.codexradar.com";

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("invalid public-summary JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported or missing schema version: {found:?}")]
    SchemaVersion { found: Option<String> },
    #[error("unexpected or missing payload type: {found:?}")]
    PayloadType { found: Option<String> },
    #[error("source contains no valid update timestamp: {value:?}")]
    Timestamp { value: Option<String> },
    #[error("source contains no candidates with a finite score and identity")]
    NoCandidates,
}

impl SourceError {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Json(_) => "json",
            Self::SchemaVersion { .. } => "schema",
            Self::PayloadType { .. } => "type",
            Self::Timestamp { .. } => "timestamp",
            Self::NoCandidates => "no_candidates",
        }
    }
}

#[derive(Debug, Deserialize)]
struct PublicSummary {
    #[serde(default)]
    schema_version: Option<String>,
    #[serde(default, rename = "type")]
    payload_type: Option<String>,
    #[serde(default)]
    api_access: Option<ApiAccess>,
    #[serde(default)]
    model_iq: Option<ModelIq>,
}

#[derive(Debug, Deserialize)]
struct ApiAccess {
    #[serde(default)]
    requirements: Option<AttributionRequirements>,
}

#[derive(Debug, Deserialize)]
struct AttributionRequirements {
    #[serde(default)]
    attribution_text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelIq {
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    latest: Option<RemoteScore>,
    #[serde(default)]
    comparisons: BTreeMap<String, Comparison>,
}

#[derive(Debug, Deserialize)]
struct Comparison {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    reasoning_effort: Option<String>,
    #[serde(default)]
    latest: Option<RemoteScore>,
}

#[derive(Debug, Default, Deserialize)]
struct RemoteScore {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    reasoning_effort: Option<String>,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    passed: Option<u64>,
    #[serde(default)]
    tasks: Option<u64>,
    #[serde(default)]
    valid_tasks: Option<u64>,
    #[serde(default)]
    average_cost_usd: Option<f64>,
    #[serde(default)]
    average_task_seconds: Option<f64>,
    #[serde(default)]
    average_task_time_human: Option<String>,
    #[serde(default)]
    wall_time_human: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DistributedTable {
    #[serde(default)]
    schema: Option<Value>,
    #[serde(default)]
    combos: Vec<DistributedCombo>,
    #[serde(default)]
    tasks: Vec<DistributedTask>,
    #[serde(default)]
    cells: BTreeMap<String, DistributedCell>,
    #[serde(default)]
    baseline_generated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DistributedCombo {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effort: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DistributedTask {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DistributedCell {
    #[serde(default)]
    last_graded_at: Option<String>,
    #[serde(default)]
    ran_by: Vec<DistributedRun>,
}

#[derive(Debug, Deserialize)]
struct DistributedRun {
    #[serde(default)]
    passed: bool,
    #[serde(default)]
    graded_at: Option<String>,
    #[serde(default)]
    duration_sec: Option<f64>,
    #[serde(default)]
    actual_cost_usd: Option<f64>,
    #[serde(default)]
    cost_complete: bool,
}

pub fn parse_snapshot(
    bytes: &[u8],
    checked_at: DateTime<Utc>,
) -> Result<RadarSnapshot, SourceError> {
    let summary: PublicSummary = serde_json::from_slice(bytes)?;

    if summary.schema_version.as_deref() != Some(SUPPORTED_SCHEMA_VERSION) {
        return Err(SourceError::SchemaVersion {
            found: summary.schema_version,
        });
    }

    if summary.payload_type.as_deref() != Some(PUBLIC_SUMMARY_TYPE) {
        return Err(SourceError::PayloadType {
            found: summary.payload_type,
        });
    }

    let attribution = normalize_attribution(summary.api_access);
    let model_iq = summary
        .model_iq
        .ok_or(SourceError::Timestamp { value: None })?;
    let timestamp_value = model_iq.updated_at;
    let updated_at = timestamp_value
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .ok_or_else(|| SourceError::Timestamp {
            value: timestamp_value.clone(),
        })?;

    let mut candidates = BTreeMap::new();

    if let Some(latest) = model_iq.latest.as_ref() {
        if let Some(candidate) = normalize_candidate(latest, None, None, None) {
            insert_candidate(&mut candidates, candidate);
        }
    }

    for comparison in model_iq.comparisons.values() {
        let Some(latest) = comparison.latest.as_ref() else {
            continue;
        };

        if let Some(candidate) = normalize_candidate(
            latest,
            comparison.label.as_deref(),
            comparison.model.as_deref(),
            comparison.reasoning_effort.as_deref(),
        ) {
            insert_candidate(&mut candidates, candidate);
        }
    }

    let mut rankings: Vec<_> = candidates.into_values().collect();
    rankings.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });

    let leader_score = rankings
        .first()
        .map(|candidate| candidate.score)
        .ok_or(SourceError::NoCandidates)?;
    let mut leader_ids: Vec<_> = rankings
        .iter()
        .take_while(|candidate| candidate.score == leader_score)
        .map(|candidate| candidate.id.clone())
        .collect();
    leader_ids.sort();

    Ok(RadarSnapshot {
        source: RadarSource::Main,
        schema_version: SUPPORTED_SCHEMA_VERSION.to_owned(),
        updated_at,
        checked_at,
        leader_ids,
        rankings,
        attribution,
        source_url: PUBLIC_SUMMARY_URL.to_owned(),
    })
}

pub fn parse_source_snapshot(
    source: RadarSource,
    bytes: &[u8],
    checked_at: DateTime<Utc>,
) -> Result<RadarSnapshot, SourceError> {
    match source {
        RadarSource::Main => parse_snapshot(bytes, checked_at),
        RadarSource::Distributed => parse_distributed_snapshot(bytes, checked_at),
    }
}

pub fn parse_distributed_snapshot(
    bytes: &[u8],
    checked_at: DateTime<Utc>,
) -> Result<RadarSnapshot, SourceError> {
    let table: DistributedTable = serde_json::from_slice(bytes)?;

    if table.schema.as_ref().and_then(Value::as_u64) != Some(DISTRIBUTED_SCHEMA_VERSION) {
        return Err(SourceError::SchemaVersion {
            found: table.schema.map(|value| value.to_string()),
        });
    }

    let timestamp_hint = table.baseline_generated_at.clone();
    let mut updated_at = None;
    include_timestamp(&mut updated_at, table.baseline_generated_at.as_deref());
    for cell in table.cells.values() {
        include_timestamp(&mut updated_at, cell.last_graded_at.as_deref());
        include_timestamp(
            &mut updated_at,
            cell.ran_by.first().and_then(|run| run.graded_at.as_deref()),
        );
    }
    let updated_at = updated_at.ok_or(SourceError::Timestamp {
        value: timestamp_hint,
    })?;

    let task_ids: Vec<_> = table
        .tasks
        .iter()
        .filter_map(|task| non_empty(task.id.as_deref()))
        .collect();
    let mut candidates = BTreeMap::new();

    for combo in &table.combos {
        let Some(model) = non_empty(combo.model.as_deref()) else {
            continue;
        };
        let Some(reasoning_effort) = non_empty(combo.effort.as_deref()) else {
            continue;
        };

        let mut passed = 0_u64;
        let mut sampled = 0_u64;
        let mut duration = MetricAverage::default();
        let mut cost = MetricAverage::default();

        for task_id in &task_ids {
            let key = format!("{task_id}|{model}|{reasoning_effort}");
            let Some(run) = table.cells.get(&key).and_then(|cell| cell.ran_by.first()) else {
                continue;
            };

            sampled += 1;
            passed += u64::from(run.passed);
            duration.observe(run.duration_sec);
            if !reasoning_effort.eq_ignore_ascii_case("ultra") || run.cost_complete {
                cost.observe(run.actual_cost_usd);
            }
        }

        if sampled == 0 {
            continue;
        }

        let score = ((passed * 150 + sampled / 2) / sampled) as f64;
        let id = format!("{model}:{reasoning_effort}");
        insert_candidate(
            &mut candidates,
            ModelScore {
                id,
                label: fallback_label(model, reasoning_effort),
                model: model.to_owned(),
                reasoning_effort: reasoning_effort.to_owned(),
                score,
                status: None,
                passed: Some(passed),
                tasks: Some(sampled),
                valid_tasks: Some(sampled),
                average_cost_usd: cost.value(),
                average_task_seconds: duration.value(),
                average_task_time_human: None,
                wall_time_human: None,
            },
        );
    }

    let mut rankings: Vec<_> = candidates.into_values().collect();
    rankings.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });

    let leader_score = rankings
        .first()
        .map(|candidate| candidate.score)
        .ok_or(SourceError::NoCandidates)?;
    let leader_ids = rankings
        .iter()
        .take_while(|candidate| candidate.score == leader_score)
        .map(|candidate| candidate.id.clone())
        .collect();

    Ok(RadarSnapshot {
        source: RadarSource::Distributed,
        schema_version: SUPPORTED_SCHEMA_VERSION.to_owned(),
        updated_at,
        checked_at,
        leader_ids,
        rankings,
        attribution: Attribution {
            text: DISTRIBUTED_ATTRIBUTION_TEXT.to_owned(),
            url: DISTRIBUTED_ATTRIBUTION_URL.to_owned(),
        },
        source_url: DISTRIBUTED_TABLE_URL.to_owned(),
    })
}

#[derive(Default)]
struct MetricAverage {
    total: f64,
    count: u64,
}

impl MetricAverage {
    fn observe(&mut self, value: Option<f64>) {
        if let Some(value) = finite_non_negative(value) {
            self.total += value;
            self.count += 1;
        }
    }

    fn value(&self) -> Option<f64> {
        (self.count > 0)
            .then(|| self.total / self.count as f64)
            .filter(|value| value.is_finite())
    }
}

fn include_timestamp(current: &mut Option<DateTime<chrono::FixedOffset>>, value: Option<&str>) {
    let Some(next) = value.and_then(|value| DateTime::parse_from_rfc3339(value).ok()) else {
        return;
    };

    if current.as_ref().is_none_or(|current| next > *current) {
        *current = Some(next);
    }
}

fn normalize_attribution(api_access: Option<ApiAccess>) -> Attribution {
    let text = api_access
        .and_then(|access| access.requirements)
        .and_then(|requirements| owned_non_empty(requirements.attribution_text.as_deref()))
        .unwrap_or_else(|| DEFAULT_ATTRIBUTION_TEXT.to_owned());

    Attribution {
        text,
        url: ATTRIBUTION_URL.to_owned(),
    }
}

fn normalize_candidate(
    score: &RemoteScore,
    label_hint: Option<&str>,
    model_hint: Option<&str>,
    reasoning_effort_hint: Option<&str>,
) -> Option<ModelScore> {
    let model = non_empty(score.model.as_deref())
        .or_else(|| non_empty(model_hint))?
        .to_owned();
    let reasoning_effort = non_empty(score.reasoning_effort.as_deref())
        .or_else(|| non_empty(reasoning_effort_hint))?
        .to_owned();
    let value = score.score.filter(|value| value.is_finite())?;
    let id = format!("{model}:{reasoning_effort}");
    let label = non_empty(label_hint)
        .or_else(|| non_empty(score.label.as_deref()))
        .map(str::to_owned)
        .unwrap_or_else(|| fallback_label(&model, &reasoning_effort));

    Some(ModelScore {
        id,
        label,
        model,
        reasoning_effort,
        score: value,
        status: owned_non_empty(score.status.as_deref()),
        passed: score.passed,
        tasks: score.tasks,
        valid_tasks: score.valid_tasks,
        average_cost_usd: finite(score.average_cost_usd),
        average_task_seconds: finite(score.average_task_seconds),
        average_task_time_human: owned_non_empty(score.average_task_time_human.as_deref()),
        wall_time_human: owned_non_empty(score.wall_time_human.as_deref()),
    })
}

fn insert_candidate(candidates: &mut BTreeMap<String, ModelScore>, candidate: ModelScore) {
    match candidates.entry(candidate.id.clone()) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(candidate);
        }
        std::collections::btree_map::Entry::Occupied(mut entry) => {
            if candidate.score.total_cmp(&entry.get().score) != Ordering::Less {
                entry.insert(candidate);
            }
        }
    }
}

fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

fn finite_non_negative(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn owned_non_empty(value: Option<&str>) -> Option<String> {
    non_empty(value).map(str::to_owned)
}

fn fallback_label(model: &str, reasoning_effort: &str) -> String {
    format!("{} {reasoning_effort}", display_model(model))
}

fn display_model(model: &str) -> String {
    let mut parts = model.split('-');
    let first = parts.next().unwrap_or(model);
    let mut display = if first.eq_ignore_ascii_case("gpt") {
        "GPT".to_owned()
    } else {
        capitalize_ascii(first)
    };

    for (index, part) in parts.enumerate() {
        if display == "GPT" && index == 0 {
            display.push('-');
            display.push_str(part);
        } else {
            display.push(' ');
            display.push_str(&capitalize_ascii(part));
        }
    }

    display
}

fn capitalize_ascii(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    if let Some(first) = bytes.first_mut() {
        first.make_ascii_uppercase();
    }
    String::from_utf8(bytes).expect("capitalizing ASCII preserves valid UTF-8")
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use serde_json::{json, Value};

    use super::*;

    fn checked_at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-07-19T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn payload(latest: Value, comparisons: Value, attribution: Option<Value>) -> Vec<u8> {
        let mut value = json!({
            "schema_version": "2.0",
            "type": "public_summary",
            "model_iq": {
                "updated_at": "2026-07-19T21:56:42+08:00",
                "latest": latest,
                "comparisons": comparisons
            }
        });

        if let Some(attribution) = attribution {
            value["api_access"] = json!({ "requirements": attribution });
        }

        serde_json::to_vec(&value).unwrap()
    }

    #[test]
    fn parses_and_ranks_primary_and_comparison_candidates() {
        let bytes = payload(
            json!({
                "model": "gpt-5.6-sol",
                "reasoning_effort": "max",
                "score": 106.3,
                "status": "green",
                "passed": 79,
                "tasks": 112,
                "valid_tasks": 112,
                "average_cost_usd": 10.276539,
                "average_task_seconds": 2383.018,
                "average_task_time_human": "40分钟",
                "wall_time_human": "74小时8分"
            }),
            json!({
                "high": {
                    "label": "  GPT-5.6 Sol high  ",
                    "model": "gpt-5.6-sol",
                    "reasoning_effort": "high",
                    "latest": {
                        "score": 96.9,
                        "status": "green",
                        "passed": 72,
                        "tasks": 112,
                        "valid_tasks": 112
                    }
                }
            }),
            Some(json!({
                "attribution_text": "  Data from Codex Radar  ",
                "site": "https://untrusted.example"
            })),
        );

        let snapshot = parse_snapshot(&bytes, checked_at()).unwrap();

        assert_eq!(snapshot.rankings.len(), 2);
        assert_eq!(snapshot.rankings[0].id, "gpt-5.6-sol:max");
        assert_eq!(snapshot.rankings[0].score, 106.3);
        assert_eq!(snapshot.rankings[0].passed, Some(79));
        assert_eq!(snapshot.rankings[1].label, "GPT-5.6 Sol high");
        assert_eq!(snapshot.leader_ids, ["gpt-5.6-sol:max"]);
        assert_eq!(snapshot.attribution.text, "Data from Codex Radar");
        assert_eq!(snapshot.attribution.url, ATTRIBUTION_URL);
        assert_eq!(snapshot.source_url, PUBLIC_SUMMARY_URL);
        assert_eq!(snapshot.source, RadarSource::Main);
        assert_eq!(snapshot.updated_at.offset().local_minus_utc(), 8 * 60 * 60);

        let serialized = serde_json::to_value(snapshot).unwrap();
        assert_eq!(serialized["source"], "main");
        assert_eq!(serialized["rankings"][0]["reasoningEffort"], "max");
        assert!(serialized.get("leaderIds").is_some());
    }

    #[test]
    fn preserves_every_tied_leader_in_stable_identity_order() {
        let bytes = payload(
            json!({
                "model": "gpt-z",
                "reasoning_effort": "max",
                "score": 100.0
            }),
            json!({
                "low": {
                    "model": "gpt-low",
                    "reasoning_effort": "high",
                    "latest": { "score": 90.0 }
                },
                "tie": {
                    "model": "gpt-a",
                    "reasoning_effort": "max",
                    "latest": { "score": 100.0 }
                }
            }),
            None,
        );

        let snapshot = parse_snapshot(&bytes, checked_at()).unwrap();

        assert_eq!(
            snapshot
                .rankings
                .iter()
                .map(|candidate| candidate.id.as_str())
                .collect::<Vec<_>>(),
            ["gpt-a:max", "gpt-z:max", "gpt-low:high"]
        );
        assert_eq!(snapshot.leader_ids, ["gpt-a:max", "gpt-z:max"]);
    }

    #[test]
    fn rejects_malformed_json() {
        let error = parse_snapshot(b"{", checked_at()).unwrap_err();

        assert!(matches!(error, SourceError::Json(_)));
    }

    #[test]
    fn classifies_schema_type_and_timestamp_errors() {
        let wrong_schema = json!({ "schema_version": "1.0", "type": "public_summary" });
        let error =
            parse_snapshot(&serde_json::to_vec(&wrong_schema).unwrap(), checked_at()).unwrap_err();
        assert!(matches!(error, SourceError::SchemaVersion { .. }));

        let wrong_type = json!({ "schema_version": "2.0", "type": "full_api" });
        let error =
            parse_snapshot(&serde_json::to_vec(&wrong_type).unwrap(), checked_at()).unwrap_err();
        assert!(matches!(error, SourceError::PayloadType { .. }));

        let bad_timestamp = json!({
            "schema_version": "2.0",
            "type": "public_summary",
            "model_iq": { "updated_at": "not-a-timestamp" }
        });
        let error =
            parse_snapshot(&serde_json::to_vec(&bad_timestamp).unwrap(), checked_at()).unwrap_err();
        assert!(matches!(error, SourceError::Timestamp { .. }));
    }

    #[test]
    fn rejects_payload_without_usable_candidates() {
        let bytes = payload(
            json!({
                "model": "gpt-5.6-sol",
                "reasoning_effort": "max",
                "score": null
            }),
            json!({
                "missing_identity": {
                    "latest": { "score": 100.0 }
                },
                "missing_score": {
                    "model": "gpt-5.5",
                    "reasoning_effort": "high",
                    "latest": {}
                }
            }),
            None,
        );

        let error = parse_snapshot(&bytes, checked_at()).unwrap_err();

        assert!(matches!(error, SourceError::NoCandidates));
    }

    #[test]
    fn filters_non_finite_scores_and_statistics() {
        let score = RemoteScore {
            model: Some("gpt-5.6-sol".to_owned()),
            reasoning_effort: Some("max".to_owned()),
            score: Some(f64::NAN),
            average_cost_usd: Some(f64::INFINITY),
            ..RemoteScore::default()
        };

        assert!(normalize_candidate(&score, None, None, None).is_none());

        let score = RemoteScore {
            score: Some(100.0),
            ..score
        };
        let candidate = normalize_candidate(&score, None, None, None).unwrap();
        assert_eq!(candidate.average_cost_usd, None);
    }

    #[test]
    fn falls_back_for_blank_labels_and_missing_attribution() {
        let bytes = payload(
            json!({
                "model": "gpt-5.6-sol",
                "reasoning_effort": "max",
                "score": 106.3
            }),
            json!({
                "comparison": {
                    "label": "   ",
                    "model": "gpt-5.5",
                    "reasoning_effort": "high",
                    "latest": { "score": 90.0 }
                }
            }),
            None,
        );

        let snapshot = parse_snapshot(&bytes, checked_at()).unwrap();

        assert_eq!(snapshot.rankings[0].label, "GPT-5.6 Sol max");
        assert_eq!(snapshot.rankings[1].label, "GPT-5.5 high");
        assert_eq!(snapshot.attribution.text, DEFAULT_ATTRIBUTION_TEXT);
        assert_eq!(snapshot.attribution.url, ATTRIBUTION_URL);
    }

    #[test]
    fn distributed_uses_latest_runs_and_matches_live_iq_rounding_and_ties() {
        let bytes = serde_json::to_vec(&json!({
            "schema": 1,
            "baseline_generated_at": "2026-07-20T00:00:00+08:00",
            "combos": [
                { "model": "gpt-alpha", "effort": "high" },
                { "model": "gpt-beta", "effort": "max" },
                { "model": "gpt-ultra", "effort": "ultra" }
            ],
            "tasks": [
                { "id": "t1" }, { "id": "t2" }, { "id": "t3" }, { "id": "t4" }
            ],
            "cells": {
                "t1|gpt-alpha|high": {
                    "last_graded_at": "2026-07-20T01:00:00+08:00",
                    "ran_by": [
                        {
                            "passed": false,
                            "graded_at": "2026-07-20T01:00:00+08:00",
                            "duration_sec": 10.0,
                            "actual_cost_usd": 1.0
                        },
                        { "passed": true, "duration_sec": 900.0, "actual_cost_usd": 900.0 }
                    ]
                },
                "t2|gpt-alpha|high": {
                    "ran_by": [{
                        "passed": true,
                        "graded_at": "2026-07-20T03:00:00+08:00",
                        "duration_sec": 20.0,
                        "actual_cost_usd": 3.0,
                        "cost_complete": false
                    }]
                },
                "t3|gpt-alpha|high": {
                    "ran_by": [{ "passed": true, "duration_sec": -5.0, "actual_cost_usd": 5.0 }]
                },
                "t4|gpt-alpha|high": {
                    "last_graded_at": "not-a-timestamp",
                    "ran_by": [{ "passed": true, "actual_cost_usd": 7.0 }]
                },
                "t1|gpt-beta|max": { "ran_by": [{ "passed": false }] },
                "t2|gpt-beta|max": { "ran_by": [{ "passed": true }] },
                "t3|gpt-beta|max": { "ran_by": [{ "passed": true }] },
                "t4|gpt-beta|max": { "ran_by": [{ "passed": true }] },
                "t1|gpt-ultra|ultra": {
                    "ran_by": [{
                        "passed": true,
                        "actual_cost_usd": 10.0,
                        "cost_complete": false
                    }]
                },
                "t2|gpt-ultra|ultra": {
                    "ran_by": [{ "passed": true, "actual_cost_usd": 20.0 }]
                },
                "t3|gpt-ultra|ultra": {
                    "ran_by": [{
                        "passed": false,
                        "actual_cost_usd": 30.0,
                        "cost_complete": true
                    }]
                }
            }
        }))
        .unwrap();

        let snapshot = parse_distributed_snapshot(&bytes, checked_at()).unwrap();

        assert_eq!(snapshot.source, RadarSource::Distributed);
        assert_eq!(snapshot.source_url, DISTRIBUTED_TABLE_URL);
        assert_eq!(snapshot.attribution.url, DISTRIBUTED_ATTRIBUTION_URL);
        assert_eq!(
            snapshot.updated_at.to_rfc3339(),
            "2026-07-20T03:00:00+08:00"
        );
        assert_eq!(snapshot.leader_ids, ["gpt-alpha:high", "gpt-beta:max"]);
        assert_eq!(
            snapshot
                .rankings
                .iter()
                .map(|candidate| (candidate.id.as_str(), candidate.score))
                .collect::<Vec<_>>(),
            [
                ("gpt-alpha:high", 113.0),
                ("gpt-beta:max", 113.0),
                ("gpt-ultra:ultra", 100.0)
            ]
        );

        let alpha = &snapshot.rankings[0];
        assert_eq!(
            (alpha.passed, alpha.tasks, alpha.valid_tasks),
            (Some(3), Some(4), Some(4))
        );
        assert_eq!(alpha.average_task_seconds, Some(15.0));
        assert_eq!(alpha.average_cost_usd, Some(4.0));

        let ultra = &snapshot.rankings[2];
        assert_eq!(ultra.average_cost_usd, Some(30.0));
        assert_eq!(ultra.status, None);
        assert_eq!(ultra.average_task_time_human, None);
        assert_eq!(ultra.wall_time_human, None);

        let serialized = serde_json::to_value(snapshot).unwrap();
        assert_eq!(serialized["source"], "distributed");
    }

    #[test]
    fn distributed_rejects_bad_schema_invalid_timestamp_set_and_no_candidates() {
        let wrong_schema = json!({
            "schema": 2,
            "baseline_generated_at": "2026-07-20T00:00:00Z",
            "combos": [],
            "tasks": [],
            "cells": {}
        });
        let error =
            parse_distributed_snapshot(&serde_json::to_vec(&wrong_schema).unwrap(), checked_at())
                .unwrap_err();
        assert!(matches!(error, SourceError::SchemaVersion { .. }));

        let bad_timestamps = json!({
            "schema": 1,
            "baseline_generated_at": "invalid",
            "combos": [{ "model": "gpt-a", "effort": "max" }],
            "tasks": [{ "id": "t1" }],
            "cells": {
                "t1|gpt-a|max": {
                    "last_graded_at": "also-invalid",
                    "ran_by": [{ "passed": true, "graded_at": "still-invalid" }]
                }
            }
        });
        let error =
            parse_distributed_snapshot(&serde_json::to_vec(&bad_timestamps).unwrap(), checked_at())
                .unwrap_err();
        assert!(matches!(error, SourceError::Timestamp { .. }));

        let no_candidates = json!({
            "schema": 1,
            "baseline_generated_at": "2026-07-20T00:00:00Z",
            "combos": [{ "model": "gpt-a", "effort": "max" }],
            "tasks": [{ "id": "t1" }],
            "cells": { "t1|gpt-a|max": { "ran_by": [] } }
        });
        let error =
            parse_distributed_snapshot(&serde_json::to_vec(&no_candidates).unwrap(), checked_at())
                .unwrap_err();
        assert!(matches!(error, SourceError::NoCandidates));
    }
}
