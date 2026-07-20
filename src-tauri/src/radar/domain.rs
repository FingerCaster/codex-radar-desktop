use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RadarSource {
    #[default]
    Main,
    Distributed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelScore {
    pub id: String,
    pub label: String,
    pub model: String,
    pub reasoning_effort: String,
    pub score: f64,
    pub status: Option<String>,
    pub passed: Option<u64>,
    pub tasks: Option<u64>,
    pub valid_tasks: Option<u64>,
    pub average_cost_usd: Option<f64>,
    pub average_task_seconds: Option<f64>,
    pub average_task_time_human: Option<String>,
    pub wall_time_human: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    pub text: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarSnapshot {
    pub source: RadarSource,
    pub schema_version: String,
    pub updated_at: DateTime<FixedOffset>,
    pub checked_at: DateTime<Utc>,
    pub leader_ids: Vec<String>,
    pub rankings: Vec<ModelScore>,
    pub attribution: Attribution,
    pub source_url: String,
}
