use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use chrono::{DateTime, FixedOffset, Utc};
use reqwest::{
    header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED},
    StatusCode,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_notification::NotificationExt;
use thiserror::Error;
use tokio::sync::{Mutex, Notify, OnceCell, RwLock};

use super::{
    domain::{RadarSnapshot, RadarSource},
    source::{parse_source_snapshot, SourceError, DISTRIBUTED_TABLE_URL, PUBLIC_SUMMARY_URL},
};

pub const SNAPSHOT_UPDATED_EVENT: &str = "radar://snapshot-updated";
pub const REFRESH_FAILED_EVENT: &str = "radar://refresh-failed";
pub const REFRESH_REQUESTED_EVENT: &str = "radar://refresh-requested";
const MAIN_POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);
const DISTRIBUTED_POLL_INTERVAL: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAIN_MAX_RESPONSE_BYTES: usize = 512 * 1024;
const DISTRIBUTED_MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshOutcome {
    pub snapshot: RadarSnapshot,
    pub not_modified: bool,
    pub leader_changed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshFailure {
    pub source: RadarSource,
    pub kind: String,
    pub message: String,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
enum RadarError {
    #[error("network request failed: {0}")]
    Network(String),
    #[error("source returned HTTP {0}")]
    Http(u16),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error("source payload is older than the current snapshot")]
    StalePayload,
    #[error("source returned not-modified before a snapshot was cached")]
    NoCachedSnapshot,
    #[error("source response exceeded the {0} byte limit")]
    ResponseTooLarge(usize),
}

impl RadarError {
    fn kind(&self) -> &'static str {
        match self {
            Self::Network(_) => "network",
            Self::Http(_) => "http",
            Self::Source(error) => error.kind(),
            Self::StalePayload => "stale_payload",
            Self::NoCachedSnapshot => "no_cache",
            Self::ResponseTooLarge(_) => "response_too_large",
        }
    }

    fn as_failure(&self, source: RadarSource) -> RefreshFailure {
        RefreshFailure {
            source,
            kind: self.kind().to_owned(),
            message: self.to_string(),
            occurred_at: Utc::now(),
        }
    }
}

impl RefreshFailure {
    fn superseded(source: RadarSource) -> Self {
        Self {
            source,
            kind: "superseded".to_owned(),
            message: "refresh was superseded by a newer source selection".to_owned(),
            occurred_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RefreshToken {
    source: RadarSource,
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveSource {
    token: RefreshToken,
    latest_main_generation: Option<u64>,
    latest_distributed_generation: Option<u64>,
    notification_baseline_ready: bool,
}

impl ActiveSource {
    const fn new(source: RadarSource) -> Self {
        Self {
            token: RefreshToken {
                source,
                generation: 0,
            },
            latest_main_generation: match source {
                RadarSource::Main => Some(0),
                RadarSource::Distributed => None,
            },
            latest_distributed_generation: match source {
                RadarSource::Main => None,
                RadarSource::Distributed => Some(0),
            },
            notification_baseline_ready: false,
        }
    }

    fn select(&mut self, source: RadarSource) -> bool {
        if self.token.source == source {
            return false;
        }

        self.token = RefreshToken {
            source,
            generation: self.token.generation.wrapping_add(1),
        };
        match source {
            RadarSource::Main => self.latest_main_generation = Some(self.token.generation),
            RadarSource::Distributed => {
                self.latest_distributed_generation = Some(self.token.generation)
            }
        }
        self.notification_baseline_ready = false;
        true
    }

    const fn is_latest_activation(&self, token: RefreshToken) -> bool {
        let latest = match token.source {
            RadarSource::Main => self.latest_main_generation,
            RadarSource::Distributed => self.latest_distributed_generation,
        };
        matches!(latest, Some(generation) if generation == token.generation)
    }

    fn accept_leader_change(&mut self, requested: bool) -> bool {
        let accepted = self.notification_baseline_ready && requested;
        self.notification_baseline_ready = true;
        accepted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CacheValidator {
    Etag(String),
    LastModified(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConditionalHeader {
    IfNoneMatch(String),
    IfModifiedSince(String),
}

impl CacheValidator {
    fn for_request(&self, source: RadarSource) -> Option<ConditionalHeader> {
        match (source, self) {
            (RadarSource::Main, Self::Etag(value)) => {
                Some(ConditionalHeader::IfNoneMatch(value.clone()))
            }
            (RadarSource::Distributed, Self::LastModified(value)) => {
                Some(ConditionalHeader::IfModifiedSince(value.clone()))
            }
            _ => None,
        }
    }

    fn from_response(source: RadarSource, headers: &reqwest::header::HeaderMap) -> Option<Self> {
        let (name, build): (_, fn(String) -> Self) = match source {
            RadarSource::Main => (ETAG, Self::Etag),
            RadarSource::Distributed => (LAST_MODIFIED, Self::LastModified),
        };

        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .map(build)
    }
}

#[derive(Default)]
struct SourceState {
    snapshot: Option<RadarSnapshot>,
    validator: Option<CacheValidator>,
}

struct SourceRuntime {
    state: RwLock<SourceState>,
    refreshes: SingleFlight<Result<RefreshOutcome, RefreshFailure>>,
}

impl Default for SourceRuntime {
    fn default() -> Self {
        Self {
            state: RwLock::new(SourceState::default()),
            refreshes: SingleFlight::default(),
        }
    }
}

#[derive(Default)]
struct SourceRuntimes {
    main: SourceRuntime,
    distributed: SourceRuntime,
}

impl SourceRuntimes {
    const fn get(&self, source: RadarSource) -> &SourceRuntime {
        match source {
            RadarSource::Main => &self.main,
            RadarSource::Distributed => &self.distributed,
        }
    }
}

struct RadarServiceInner {
    client: reqwest::Client,
    active_source: RwLock<ActiveSource>,
    runtimes: SourceRuntimes,
    polling_wake: Notify,
}

struct SingleFlight<T> {
    active: Mutex<HashMap<u64, Arc<OnceCell<T>>>>,
}

impl<T> Default for SingleFlight<T> {
    fn default() -> Self {
        Self {
            active: Mutex::new(HashMap::new()),
        }
    }
}

impl<T: Clone> SingleFlight<T> {
    async fn run<F, Fut>(&self, generation: u64, work: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let flight = {
            let mut active = self.active.lock().await;
            active
                .entry(generation)
                .or_insert_with(|| Arc::new(OnceCell::new()))
                .clone()
        };

        let result = flight.get_or_init(work).await.clone();

        let mut active = self.active.lock().await;
        if active
            .get(&generation)
            .is_some_and(|current| Arc::ptr_eq(current, &flight))
        {
            active.remove(&generation);
        }

        result
    }
}

#[derive(Clone)]
pub struct RadarService {
    inner: Arc<RadarServiceInner>,
}

impl RadarService {
    pub fn new(initial_source: RadarSource) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .user_agent("ModelRadar/0.1 (+https://codexradar.com)")
            .build()
            .map_err(|error| format!("failed to build HTTP client: {error}"))?;

        Ok(Self {
            inner: Arc::new(RadarServiceInner {
                client,
                active_source: RwLock::new(ActiveSource::new(initial_source)),
                runtimes: SourceRuntimes::default(),
                polling_wake: Notify::new(),
            }),
        })
    }

    pub async fn active_source(&self) -> RadarSource {
        self.inner.active_source.read().await.token.source
    }

    pub async fn transition_source<T, F>(
        &self,
        next: RadarSource,
        commit_preference: F,
    ) -> Result<(T, bool), String>
    where
        T: Send,
        F: FnOnce() -> Result<T, String> + Send,
    {
        let mut active = self.inner.active_source.write().await;
        let committed = commit_preference()?;
        let changed = active.select(next);
        Ok((committed, changed))
    }

    pub fn wake_polling(&self) {
        self.inner.polling_wake.notify_one();
    }

    pub async fn snapshot(&self) -> Option<RadarSnapshot> {
        let active = self.inner.active_source.read().await;
        self.runtime(active.token.source)
            .state
            .read()
            .await
            .snapshot
            .clone()
    }

    async fn refresh_token(&self) -> RefreshToken {
        self.inner.active_source.read().await.token
    }

    pub async fn refresh_and_publish(
        &self,
        app: &AppHandle,
    ) -> Result<RefreshOutcome, RefreshFailure> {
        let token = self.refresh_token().await;
        let result = self
            .runtime(token.source)
            .refreshes
            .run(token.generation, || async {
                if self.refresh_token().await != token {
                    return Err(RefreshFailure::superseded(token.source));
                }

                let result = self
                    .refresh_once(token)
                    .await
                    .map_err(|error| error.as_failure(token.source));

                // The write guard linearizes publication and notification state
                // with source transitions.
                let mut active = self.inner.active_source.write().await;
                if !should_publish(active.token, token) {
                    return Err(RefreshFailure::superseded(token.source));
                }

                match result {
                    Ok(mut outcome) => {
                        outcome.leader_changed =
                            active.accept_leader_change(outcome.leader_changed);
                        let _ = app.emit(SNAPSHOT_UPDATED_EVENT, &outcome.snapshot);
                        if outcome.leader_changed {
                            show_leader_notification(app, &outcome.snapshot);
                        }
                        Ok(outcome)
                    }
                    Err(failure) => {
                        eprintln!("[model-radar] {}", failure.message);
                        let _ = app.emit(REFRESH_FAILED_EVENT, &failure);
                        Err(failure)
                    }
                }
            })
            .await;
        self.finish_refresh_caller(token, result).await
    }

    async fn finish_refresh_caller<T>(
        &self,
        token: RefreshToken,
        result: Result<T, RefreshFailure>,
    ) -> Result<T, RefreshFailure> {
        if self.refresh_token().await != token {
            return Err(RefreshFailure::superseded(token.source));
        }
        result
    }

    fn runtime(&self, source: RadarSource) -> &SourceRuntime {
        self.inner.runtimes.get(source)
    }

    async fn refresh_once(&self, token: RefreshToken) -> Result<RefreshOutcome, RadarError> {
        let runtime = self.runtime(token.source);
        let mut request = self.inner.client.get(endpoint(token.source));

        if let Some(header) = runtime
            .state
            .read()
            .await
            .validator
            .as_ref()
            .and_then(|validator| validator.for_request(token.source))
        {
            request = match header {
                ConditionalHeader::IfNoneMatch(value) => request.header(IF_NONE_MATCH, value),
                ConditionalHeader::IfModifiedSince(value) => {
                    request.header(IF_MODIFIED_SINCE, value)
                }
            };
        }

        let mut response = request
            .send()
            .await
            .map_err(|error| RadarError::Network(error.to_string()))?;
        let next_validator = CacheValidator::from_response(token.source, response.headers());

        if response.status() == StatusCode::NOT_MODIFIED {
            return self
                .commit_not_modified(token, next_validator, Utc::now())
                .await;
        }

        if !response.status().is_success() {
            return Err(RadarError::Http(response.status().as_u16()));
        }

        let response_limit = response_limit(token.source);
        if response
            .content_length()
            .is_some_and(|length| length > response_limit as u64)
        {
            return Err(RadarError::ResponseTooLarge(response_limit));
        }

        let mut bytes = Vec::with_capacity(
            response
                .content_length()
                .unwrap_or(0)
                .min(response_limit as u64) as usize,
        );
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| RadarError::Network(error.to_string()))?
        {
            append_limited(&mut bytes, &chunk, response_limit)?;
        }

        let next = parse_source_snapshot(token.source, &bytes, Utc::now())?;
        self.commit_snapshot(token, next, next_validator).await
    }

    async fn commit_not_modified(
        &self,
        token: RefreshToken,
        next_validator: Option<CacheValidator>,
        checked_at: DateTime<Utc>,
    ) -> Result<RefreshOutcome, RadarError> {
        let active = self.inner.active_source.read().await;
        let runtime = self.runtime(token.source);
        if !active.is_latest_activation(token) {
            let snapshot = runtime
                .state
                .read()
                .await
                .snapshot
                .clone()
                .ok_or(RadarError::NoCachedSnapshot)?;
            return Ok(RefreshOutcome {
                snapshot,
                not_modified: true,
                leader_changed: false,
            });
        }

        let mut state = runtime.state.write().await;
        let snapshot = state
            .snapshot
            .as_mut()
            .ok_or(RadarError::NoCachedSnapshot)?;
        snapshot.checked_at = checked_at;
        let snapshot = snapshot.clone();
        if let Some(next_validator) = next_validator {
            state.validator = Some(next_validator);
        }

        Ok(RefreshOutcome {
            snapshot,
            not_modified: true,
            leader_changed: false,
        })
    }

    async fn commit_snapshot(
        &self,
        token: RefreshToken,
        next: RadarSnapshot,
        next_validator: Option<CacheValidator>,
    ) -> Result<RefreshOutcome, RadarError> {
        let active = self.inner.active_source.read().await;
        let runtime = self.runtime(token.source);
        if !active.is_latest_activation(token) {
            return Ok(RefreshOutcome {
                snapshot: next,
                not_modified: false,
                leader_changed: false,
            });
        }

        let mut state = runtime.state.write().await;
        if state
            .snapshot
            .as_ref()
            .is_some_and(|current| is_older(next.updated_at, current.updated_at))
        {
            return Err(RadarError::StalePayload);
        }

        let leader_changed = leader_ids_changed(
            state
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.leader_ids.as_slice()),
            &next.leader_ids,
        );
        state.snapshot = Some(next.clone());
        state.validator = next_validator;

        Ok(RefreshOutcome {
            snapshot: next,
            not_modified: false,
            leader_changed,
        })
    }
}

#[tauri::command]
pub async fn get_radar_snapshot(
    state: State<'_, RadarService>,
) -> Result<Option<RadarSnapshot>, String> {
    let service = state.inner().clone();
    Ok(service.snapshot().await)
}

#[tauri::command]
pub async fn refresh_radar(
    app: AppHandle,
    state: State<'_, RadarService>,
) -> Result<RefreshOutcome, RefreshFailure> {
    state.refresh_and_publish(&app).await
}

pub fn start_background_polling(app: AppHandle, service: RadarService) {
    tauri::async_runtime::spawn(async move {
        loop {
            let _ = service.refresh_and_publish(&app).await;
            let interval = poll_interval(service.active_source().await);
            let _ = tokio::time::timeout(interval, service.inner.polling_wake.notified()).await;
        }
    });
}

const fn endpoint(source: RadarSource) -> &'static str {
    match source {
        RadarSource::Main => PUBLIC_SUMMARY_URL,
        RadarSource::Distributed => DISTRIBUTED_TABLE_URL,
    }
}

const fn response_limit(source: RadarSource) -> usize {
    match source {
        RadarSource::Main => MAIN_MAX_RESPONSE_BYTES,
        RadarSource::Distributed => DISTRIBUTED_MAX_RESPONSE_BYTES,
    }
}

const fn poll_interval(source: RadarSource) -> Duration {
    match source {
        RadarSource::Main => MAIN_POLL_INTERVAL,
        RadarSource::Distributed => DISTRIBUTED_POLL_INTERVAL,
    }
}

fn should_publish(active: RefreshToken, captured: RefreshToken) -> bool {
    active == captured
}

fn leader_ids_changed(previous: Option<&[String]>, next: &[String]) -> bool {
    let Some(previous) = previous else {
        return false;
    };

    let mut previous = previous.to_vec();
    let mut next = next.to_vec();
    previous.sort_unstable();
    previous.dedup();
    next.sort_unstable();
    next.dedup();
    previous != next
}

fn is_older(next: DateTime<FixedOffset>, current: DateTime<FixedOffset>) -> bool {
    next < current
}

fn append_limited(buffer: &mut Vec<u8>, chunk: &[u8], limit: usize) -> Result<(), RadarError> {
    if buffer.len().saturating_add(chunk.len()) > limit {
        return Err(RadarError::ResponseTooLarge(limit));
    }
    buffer.extend_from_slice(chunk);
    Ok(())
}

fn show_leader_notification(app: &AppHandle, snapshot: &RadarSnapshot) {
    let leaders: Vec<_> = snapshot
        .rankings
        .iter()
        .filter(|model| snapshot.leader_ids.contains(&model.id))
        .collect();
    let Some(first) = leaders.first() else {
        return;
    };

    let body = if leaders.len() == 1 {
        format!("{} 以 IQ {:.1} 成为当前榜首", first.label, first.score)
    } else {
        format!("{} 个模型以 IQ {:.1} 并列榜首", leaders.len(), first.score)
    };

    if let Err(error) = app
        .notification()
        .builder()
        .title("Model Radar 榜首更新")
        .body(body)
        .show()
    {
        eprintln!("[model-radar] notification unavailable: {error}");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use chrono::{DateTime, Utc};
    use tokio::sync::{Notify, RwLock};

    use super::*;
    use crate::radar::domain::{Attribution, ModelScore};

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    const fn token(source: RadarSource, generation: u64) -> RefreshToken {
        RefreshToken { source, generation }
    }

    fn snapshot(source: RadarSource, updated_at: &str, leaders: &[&str]) -> RadarSnapshot {
        let leader_ids = ids(leaders);
        let rankings = leader_ids
            .iter()
            .map(|id| {
                let (model, effort) = id.split_once(':').unwrap();
                ModelScore {
                    id: id.clone(),
                    label: id.clone(),
                    model: model.to_owned(),
                    reasoning_effort: effort.to_owned(),
                    score: 100.0,
                    status: None,
                    passed: None,
                    tasks: None,
                    valid_tasks: None,
                    average_cost_usd: None,
                    average_task_seconds: None,
                    average_task_time_human: None,
                    wall_time_human: None,
                }
            })
            .collect();

        RadarSnapshot {
            source,
            schema_version: "2.0".to_owned(),
            updated_at: DateTime::parse_from_rfc3339(updated_at).unwrap(),
            checked_at: Utc::now(),
            leader_ids,
            rankings,
            attribution: Attribution {
                text: "source".to_owned(),
                url: "https://example.com".to_owned(),
            },
            source_url: "https://example.com/data".to_owned(),
        }
    }

    async fn set_source_state(
        service: &RadarService,
        source: RadarSource,
        snapshot: Option<RadarSnapshot>,
        validator: Option<CacheValidator>,
    ) {
        let mut state = service.runtime(source).state.write().await;
        state.snapshot = snapshot;
        state.validator = validator;
    }

    #[test]
    fn radar_source_serializes_to_the_persisted_wire_values() {
        assert_eq!(
            serde_json::to_string(&RadarSource::Main).unwrap(),
            "\"main\""
        );
        assert_eq!(
            serde_json::to_string(&RadarSource::Distributed).unwrap(),
            "\"distributed\""
        );
        assert_eq!(
            serde_json::from_str::<RadarSource>("\"main\"").unwrap(),
            RadarSource::Main
        );
    }

    #[test]
    fn first_snapshot_does_not_count_as_a_leader_change() {
        assert!(!leader_ids_changed(None, &ids(&["gpt:max"])));
    }

    #[test]
    fn leader_comparison_uses_a_set_not_payload_order() {
        assert!(!leader_ids_changed(
            Some(&ids(&["gpt:max", "gpt:xhigh"])),
            &ids(&["gpt:xhigh", "gpt:max"]),
        ));
        assert!(leader_ids_changed(
            Some(&ids(&["gpt:max"])),
            &ids(&["gpt:xhigh"]),
        ));
    }

    #[test]
    fn older_source_timestamps_are_rejected() {
        let current = DateTime::parse_from_rfc3339("2026-07-19T21:56:42+08:00").unwrap();
        let older = DateTime::parse_from_rfc3339("2026-07-19T20:00:00+08:00").unwrap();
        assert!(is_older(older, current));
        assert!(!is_older(current, current));
    }

    #[test]
    fn request_policy_is_source_specific() {
        assert_eq!(endpoint(RadarSource::Main), PUBLIC_SUMMARY_URL);
        assert_eq!(endpoint(RadarSource::Distributed), DISTRIBUTED_TABLE_URL);
        assert_eq!(response_limit(RadarSource::Main), 512 * 1024);
        assert_eq!(response_limit(RadarSource::Distributed), 4 * 1024 * 1024);
        assert_eq!(poll_interval(RadarSource::Main), Duration::from_secs(300));
        assert_eq!(
            poll_interval(RadarSource::Distributed),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn validators_never_cross_source_request_headers() {
        let etag = CacheValidator::Etag("main-tag".to_owned());
        assert_eq!(
            etag.for_request(RadarSource::Main),
            Some(ConditionalHeader::IfNoneMatch("main-tag".to_owned()))
        );
        assert_eq!(etag.for_request(RadarSource::Distributed), None);

        let modified = CacheValidator::LastModified("Sun, 19 Jul 2026 10:00:00 GMT".to_owned());
        assert_eq!(modified.for_request(RadarSource::Main), None);
        assert_eq!(
            modified.for_request(RadarSource::Distributed),
            Some(ConditionalHeader::IfModifiedSince(
                "Sun, 19 Jul 2026 10:00:00 GMT".to_owned()
            ))
        );
    }

    #[test]
    fn response_validators_are_selected_by_source() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(ETAG, reqwest::header::HeaderValue::from_static("main-tag"));
        headers.insert(
            LAST_MODIFIED,
            reqwest::header::HeaderValue::from_static("Sun, 19 Jul 2026 10:00:00 GMT"),
        );

        assert_eq!(
            CacheValidator::from_response(RadarSource::Main, &headers),
            Some(CacheValidator::Etag("main-tag".to_owned()))
        );
        assert_eq!(
            CacheValidator::from_response(RadarSource::Distributed, &headers),
            Some(CacheValidator::LastModified(
                "Sun, 19 Jul 2026 10:00:00 GMT".to_owned()
            ))
        );
    }

    #[test]
    fn response_chunks_use_the_selected_source_limit() {
        let mut main = vec![0; MAIN_MAX_RESPONSE_BYTES];
        assert!(matches!(
            append_limited(&mut main, &[1], MAIN_MAX_RESPONSE_BYTES),
            Err(RadarError::ResponseTooLarge(MAIN_MAX_RESPONSE_BYTES))
        ));

        let mut distributed = vec![0; MAIN_MAX_RESPONSE_BYTES];
        append_limited(&mut distributed, &[1], DISTRIBUTED_MAX_RESPONSE_BYTES).unwrap();
        assert_eq!(distributed.len(), MAIN_MAX_RESPONSE_BYTES + 1);
    }

    #[test]
    fn source_switches_expose_only_that_sources_private_snapshot() {
        tauri::async_runtime::block_on(async {
            let service = RadarService::new(RadarSource::Main).unwrap();
            set_source_state(
                &service,
                RadarSource::Main,
                Some(snapshot(
                    RadarSource::Main,
                    "2026-07-20T00:00:00Z",
                    &["main:max"],
                )),
                None,
            )
            .await;
            set_source_state(
                &service,
                RadarSource::Distributed,
                Some(snapshot(
                    RadarSource::Distributed,
                    "2026-07-20T01:00:00Z",
                    &["distributed:max"],
                )),
                None,
            )
            .await;

            assert_eq!(service.snapshot().await.unwrap().source, RadarSource::Main);
            let (_, changed) = service
                .transition_source(RadarSource::Distributed, || Ok(()))
                .await
                .unwrap();
            assert!(changed);
            assert_eq!(service.active_source().await, RadarSource::Distributed);
            assert_eq!(
                service.snapshot().await.unwrap().source,
                RadarSource::Distributed
            );
        });
    }

    #[test]
    fn source_transition_is_atomic_with_preference_commit() {
        tauri::async_runtime::block_on(async {
            let service = RadarService::new(RadarSource::Main).unwrap();
            let before = service.refresh_token().await;

            let error = service
                .transition_source(RadarSource::Distributed, || {
                    assert!(service.inner.active_source.try_read().is_err());
                    Err::<(), _>("preference write failed".to_owned())
                })
                .await
                .unwrap_err();
            assert_eq!(error, "preference write failed");
            assert_eq!(service.refresh_token().await, before);

            let (value, changed) = service
                .transition_source(RadarSource::Distributed, || Ok("saved"))
                .await
                .unwrap();
            let after = service.refresh_token().await;
            assert_eq!(value, "saved");
            assert!(changed);
            assert_eq!(after.source, RadarSource::Distributed);
            assert_eq!(after.generation, before.generation + 1);
        });
    }

    #[test]
    fn activation_epoch_suppresses_its_first_leader_notification() {
        let mut active = ActiveSource::new(RadarSource::Main);
        assert!(!active.accept_leader_change(true));
        assert!(active.accept_leader_change(true));

        assert!(!active.select(RadarSource::Main));
        assert!(active.accept_leader_change(true));

        assert!(active.select(RadarSource::Distributed));
        assert!(!active.accept_leader_change(true));
        assert!(active.accept_leader_change(true));
    }

    #[test]
    fn reactivated_source_rejects_commits_from_its_old_epoch() {
        tauri::async_runtime::block_on(async {
            let service = RadarService::new(RadarSource::Main).unwrap();
            let old_main = service.refresh_token().await;
            let baseline = snapshot(RadarSource::Main, "2026-07-20T00:00:00Z", &["main:max"]);
            set_source_state(&service, RadarSource::Main, Some(baseline.clone()), None).await;

            service
                .transition_source(RadarSource::Distributed, || Ok(()))
                .await
                .unwrap();
            service
                .transition_source(RadarSource::Main, || Ok(()))
                .await
                .unwrap();
            let new_main = service.refresh_token().await;
            assert_ne!(old_main, new_main);
            assert_eq!(old_main.source, new_main.source);

            service
                .commit_snapshot(
                    old_main,
                    snapshot(
                        RadarSource::Main,
                        "2026-07-20T01:00:00Z",
                        &["old-epoch:max"],
                    ),
                    Some(CacheValidator::Etag("old-epoch".to_owned())),
                )
                .await
                .unwrap();

            let state = service.runtime(RadarSource::Main).state.read().await;
            assert_eq!(state.snapshot.as_ref(), Some(&baseline));
            assert_eq!(state.validator, None);
        });
    }

    #[test]
    fn old_epoch_stays_superseded_after_reactivated_source_is_deselected_again() {
        tauri::async_runtime::block_on(async {
            let service = RadarService::new(RadarSource::Main).unwrap();
            let old_main = service.refresh_token().await;
            let baseline = snapshot(RadarSource::Main, "2026-07-20T00:00:00Z", &["main:max"]);
            set_source_state(
                &service,
                RadarSource::Main,
                Some(baseline.clone()),
                Some(CacheValidator::Etag("baseline".to_owned())),
            )
            .await;

            service
                .transition_source(RadarSource::Distributed, || Ok(()))
                .await
                .unwrap();
            service
                .transition_source(RadarSource::Main, || Ok(()))
                .await
                .unwrap();
            let current_main = service.refresh_token().await;
            service
                .transition_source(RadarSource::Distributed, || Ok(()))
                .await
                .unwrap();

            service
                .commit_snapshot(
                    old_main,
                    snapshot(
                        RadarSource::Main,
                        "2026-07-20T01:00:00Z",
                        &["old-epoch:max"],
                    ),
                    Some(CacheValidator::Etag("old-epoch".to_owned())),
                )
                .await
                .unwrap();
            {
                let state = service.runtime(RadarSource::Main).state.read().await;
                assert_eq!(state.snapshot.as_ref(), Some(&baseline));
                assert_eq!(
                    state.validator,
                    Some(CacheValidator::Etag("baseline".to_owned()))
                );
            }

            let latest = snapshot(
                RadarSource::Main,
                "2026-07-20T02:00:00Z",
                &["current-epoch:max"],
            );
            let latest_checked_at = latest.checked_at;
            service
                .commit_snapshot(
                    current_main,
                    latest.clone(),
                    Some(CacheValidator::Etag("current-epoch".to_owned())),
                )
                .await
                .unwrap();
            service
                .commit_not_modified(
                    old_main,
                    Some(CacheValidator::Etag("old-epoch-304".to_owned())),
                    latest_checked_at + chrono::Duration::hours(1),
                )
                .await
                .unwrap();

            let state = service.runtime(RadarSource::Main).state.read().await;
            assert_eq!(state.snapshot.as_ref(), Some(&latest));
            assert_eq!(
                state.snapshot.as_ref().unwrap().checked_at,
                latest_checked_at
            );
            assert_eq!(
                state.validator,
                Some(CacheValidator::Etag("current-epoch".to_owned()))
            );
        });
    }

    #[test]
    fn not_modified_requires_a_cache_from_the_same_source() {
        tauri::async_runtime::block_on(async {
            let service = RadarService::new(RadarSource::Main).unwrap();
            set_source_state(
                &service,
                RadarSource::Main,
                Some(snapshot(
                    RadarSource::Main,
                    "2026-07-20T00:00:00Z",
                    &["main:max"],
                )),
                Some(CacheValidator::Etag("main-tag".to_owned())),
            )
            .await;

            let error = service
                .commit_not_modified(
                    token(RadarSource::Distributed, 0),
                    Some(CacheValidator::LastModified("distributed-time".to_owned())),
                    Utc::now(),
                )
                .await
                .unwrap_err();
            assert!(matches!(error, RadarError::NoCachedSnapshot));
            assert_eq!(
                service
                    .runtime(RadarSource::Distributed)
                    .state
                    .read()
                    .await
                    .validator,
                None
            );
            assert_eq!(
                service
                    .runtime(RadarSource::Main)
                    .state
                    .read()
                    .await
                    .validator,
                Some(CacheValidator::Etag("main-tag".to_owned()))
            );
        });
    }

    #[test]
    fn same_source_not_modified_updates_only_its_check_and_validator() {
        tauri::async_runtime::block_on(async {
            let service = RadarService::new(RadarSource::Distributed).unwrap();
            let distributed = snapshot(
                RadarSource::Distributed,
                "2026-07-20T01:00:00Z",
                &["distributed:max"],
            );
            let original_updated_at = distributed.updated_at;
            let checked_at = DateTime::parse_from_rfc3339("2026-07-20T02:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            set_source_state(
                &service,
                RadarSource::Distributed,
                Some(distributed),
                Some(CacheValidator::LastModified("old".to_owned())),
            )
            .await;

            let outcome = service
                .commit_not_modified(
                    service.refresh_token().await,
                    Some(CacheValidator::LastModified("new".to_owned())),
                    checked_at,
                )
                .await
                .unwrap();

            assert!(outcome.not_modified);
            assert!(!outcome.leader_changed);
            assert_eq!(outcome.snapshot.source, RadarSource::Distributed);
            assert_eq!(outcome.snapshot.updated_at, original_updated_at);
            assert_eq!(outcome.snapshot.checked_at, checked_at);
            assert_eq!(
                service
                    .runtime(RadarSource::Distributed)
                    .state
                    .read()
                    .await
                    .validator,
                Some(CacheValidator::LastModified("new".to_owned()))
            );
            assert!(service
                .runtime(RadarSource::Main)
                .state
                .read()
                .await
                .snapshot
                .is_none());
        });
    }

    #[test]
    fn stale_comparison_and_leader_baselines_are_isolated_by_source() {
        tauri::async_runtime::block_on(async {
            let service = RadarService::new(RadarSource::Main).unwrap();
            let main_token = service.refresh_token().await;
            set_source_state(
                &service,
                RadarSource::Main,
                Some(snapshot(
                    RadarSource::Main,
                    "2026-07-20T03:00:00Z",
                    &["main:max"],
                )),
                None,
            )
            .await;

            service
                .transition_source(RadarSource::Distributed, || Ok(()))
                .await
                .unwrap();
            let distributed_token = service.refresh_token().await;

            let distributed = service
                .commit_snapshot(
                    distributed_token,
                    snapshot(
                        RadarSource::Distributed,
                        "2026-07-20T01:00:00Z",
                        &["distributed:max"],
                    ),
                    Some(CacheValidator::LastModified("time".to_owned())),
                )
                .await
                .unwrap();
            assert!(!distributed.leader_changed);

            let error = service
                .commit_snapshot(
                    main_token,
                    snapshot(RadarSource::Main, "2026-07-20T02:00:00Z", &["other:max"]),
                    Some(CacheValidator::Etag("new-tag".to_owned())),
                )
                .await
                .unwrap_err();
            assert!(matches!(error, RadarError::StalePayload));
            assert_eq!(
                service
                    .runtime(RadarSource::Main)
                    .state
                    .read()
                    .await
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .leader_ids,
                ["main:max"]
            );
        });
    }

    #[test]
    fn late_results_publish_only_for_the_captured_active_source() {
        assert!(should_publish(
            token(RadarSource::Main, 2),
            token(RadarSource::Main, 2)
        ));
        assert!(!should_publish(
            token(RadarSource::Distributed, 1),
            token(RadarSource::Main, 0)
        ));
        assert!(!should_publish(
            token(RadarSource::Main, 2),
            token(RadarSource::Main, 0)
        ));
    }

    #[test]
    fn concurrent_failure_is_shared_without_replacing_cached_data() {
        tauri::async_runtime::block_on(async {
            let gate: Arc<SingleFlight<Result<(), RefreshFailure>>> =
                Arc::new(SingleFlight::default());
            let calls = Arc::new(AtomicUsize::new(0));
            let cached = Arc::new(RwLock::new(Some("last-known-good".to_owned())));
            let started = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());

            let first = {
                let gate = Arc::clone(&gate);
                let calls = Arc::clone(&calls);
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                tauri::async_runtime::spawn(async move {
                    gate.run(0, || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        started.notify_one();
                        release.notified().await;
                        Err(RefreshFailure {
                            source: RadarSource::Main,
                            kind: "network".to_owned(),
                            message: "offline".to_owned(),
                            occurred_at: Utc::now(),
                        })
                    })
                    .await
                })
            };

            started.notified().await;
            let second = {
                let gate = Arc::clone(&gate);
                let calls = Arc::clone(&calls);
                tauri::async_runtime::spawn(async move {
                    gate.run(0, || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Err(RefreshFailure {
                            source: RadarSource::Distributed,
                            kind: "unexpected".to_owned(),
                            message: "joined refresh ran twice".to_owned(),
                            occurred_at: Utc::now(),
                        })
                    })
                    .await
                })
            };

            let mut joined = false;
            for _ in 0..100 {
                let strong_count = {
                    let active = gate.active.lock().await;
                    active.get(&0).map_or(0, Arc::strong_count)
                };
                if strong_count >= 3 {
                    joined = true;
                    break;
                }
                tokio::task::yield_now().await;
            }
            assert!(joined, "second caller did not join the active refresh");
            release.notify_one();

            let first_error = first.await.unwrap().unwrap_err();
            let second_error = second.await.unwrap().unwrap_err();
            assert_eq!(first_error.source, RadarSource::Main);
            assert_eq!(second_error.source, RadarSource::Main);
            assert_eq!(first_error.kind, "network");
            assert_eq!(second_error.message, first_error.message);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(cached.read().await.as_deref(), Some("last-known-good"));
        });
    }

    #[test]
    fn source_runtimes_do_not_share_single_flights() {
        tauri::async_runtime::block_on(async {
            let service = RadarService::new(RadarSource::Main).unwrap();
            let started = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());

            let main = {
                let service = service.clone();
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                tauri::async_runtime::spawn(async move {
                    service
                        .runtime(RadarSource::Main)
                        .refreshes
                        .run(0, || async move {
                            started.notify_one();
                            release.notified().await;
                            Err(RefreshFailure {
                                source: RadarSource::Main,
                                kind: "network".to_owned(),
                                message: "main".to_owned(),
                                occurred_at: Utc::now(),
                            })
                        })
                        .await
                })
            };
            started.notified().await;

            let distributed = service
                .runtime(RadarSource::Distributed)
                .refreshes
                .run(0, || async {
                    Err(RefreshFailure {
                        source: RadarSource::Distributed,
                        kind: "network".to_owned(),
                        message: "distributed".to_owned(),
                        occurred_at: Utc::now(),
                    })
                })
                .await
                .unwrap_err();
            assert_eq!(distributed.source, RadarSource::Distributed);
            assert_eq!(distributed.message, "distributed");

            release.notify_one();
            assert_eq!(main.await.unwrap().unwrap_err().source, RadarSource::Main);
        });
    }

    #[test]
    fn a_new_activation_does_not_join_the_same_sources_old_flight() {
        tauri::async_runtime::block_on(async {
            let gate = Arc::new(SingleFlight::default());
            let calls = Arc::new(AtomicUsize::new(0));
            let started = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());

            let old = {
                let gate = Arc::clone(&gate);
                let calls = Arc::clone(&calls);
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                tauri::async_runtime::spawn(async move {
                    gate.run(0, || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        started.notify_one();
                        release.notified().await;
                        "old"
                    })
                    .await
                })
            };

            started.notified().await;
            let current = gate
                .run(2, || async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    "current"
                })
                .await;
            assert_eq!(current, "current");
            assert_eq!(calls.load(Ordering::SeqCst), 2);

            release.notify_one();
            assert_eq!(old.await.unwrap(), "old");
        });
    }

    #[test]
    fn a_delayed_old_activation_cannot_disrupt_current_joiners() {
        tauri::async_runtime::block_on(async {
            let gate = Arc::new(SingleFlight::default());
            let current_calls = Arc::new(AtomicUsize::new(0));
            let current_started = Arc::new(Notify::new());
            let release_current = Arc::new(Notify::new());

            let current = {
                let gate = Arc::clone(&gate);
                let current_calls = Arc::clone(&current_calls);
                let current_started = Arc::clone(&current_started);
                let release_current = Arc::clone(&release_current);
                tauri::async_runtime::spawn(async move {
                    gate.run(2, || async move {
                        current_calls.fetch_add(1, Ordering::SeqCst);
                        current_started.notify_one();
                        release_current.notified().await;
                        "current"
                    })
                    .await
                })
            };
            current_started.notified().await;

            let delayed_old = gate.run(0, || async { "old" }).await;
            assert_eq!(delayed_old, "old");

            let joined_current = {
                let gate = Arc::clone(&gate);
                let current_calls = Arc::clone(&current_calls);
                tauri::async_runtime::spawn(async move {
                    gate.run(2, || async move {
                        current_calls.fetch_add(1, Ordering::SeqCst);
                        "duplicate"
                    })
                    .await
                })
            };

            let mut joined = false;
            for _ in 0..100 {
                let strong_count = {
                    let active = gate.active.lock().await;
                    active.get(&2).map_or(0, Arc::strong_count)
                };
                if strong_count >= 3 {
                    joined = true;
                    break;
                }
                tokio::task::yield_now().await;
            }
            assert!(joined, "current caller did not join generation 2");

            release_current.notify_one();
            assert_eq!(current.await.unwrap(), "current");
            assert_eq!(joined_current.await.unwrap(), "current");
            assert_eq!(current_calls.load(Ordering::SeqCst), 1);
        });
    }

    #[test]
    fn each_late_joined_caller_rechecks_its_activation_before_returning() {
        tauri::async_runtime::block_on(async {
            let service = RadarService::new(RadarSource::Main).unwrap();
            let old_main = service.refresh_token().await;
            let gate = Arc::new(SingleFlight::default());
            let calls = Arc::new(AtomicUsize::new(0));
            let started = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());
            let shared_ready = Arc::new(Notify::new());
            let finish_caller = Arc::new(Notify::new());

            let initializer = {
                let gate = Arc::clone(&gate);
                let calls = Arc::clone(&calls);
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                tauri::async_runtime::spawn(async move {
                    gate.run(old_main.generation, || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        started.notify_one();
                        release.notified().await;
                        Ok::<_, RefreshFailure>("old-result")
                    })
                    .await
                })
            };
            started.notified().await;

            let joined = {
                let service = service.clone();
                let gate = Arc::clone(&gate);
                let calls = Arc::clone(&calls);
                let shared_ready = Arc::clone(&shared_ready);
                let finish_caller = Arc::clone(&finish_caller);
                tauri::async_runtime::spawn(async move {
                    let shared = gate
                        .run(old_main.generation, || async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            Err(RefreshFailure {
                                source: RadarSource::Main,
                                kind: "unexpected".to_owned(),
                                message: "joined caller started duplicate work".to_owned(),
                                occurred_at: Utc::now(),
                            })
                        })
                        .await;
                    shared_ready.notify_one();
                    finish_caller.notified().await;
                    service.finish_refresh_caller(old_main, shared).await
                })
            };

            let mut did_join = false;
            for _ in 0..100 {
                let strong_count = {
                    let active = gate.active.lock().await;
                    active
                        .get(&old_main.generation)
                        .map_or(0, Arc::strong_count)
                };
                if strong_count >= 3 {
                    did_join = true;
                    break;
                }
                tokio::task::yield_now().await;
            }
            assert!(did_join, "caller did not join the old activation flight");

            release.notify_one();
            shared_ready.notified().await;
            service
                .transition_source(RadarSource::Distributed, || Ok(()))
                .await
                .unwrap();
            service
                .transition_source(RadarSource::Main, || Ok(()))
                .await
                .unwrap();
            finish_caller.notify_one();

            assert_eq!(initializer.await.unwrap().unwrap(), "old-result");
            let failure = joined.await.unwrap().unwrap_err();
            assert_eq!(failure.kind, "superseded");
            assert_eq!(failure.source, RadarSource::Main);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        });
    }

    #[test]
    fn superseded_failures_are_source_tagged() {
        let failure = RefreshFailure::superseded(RadarSource::Main);
        assert_eq!(failure.source, RadarSource::Main);
        assert_eq!(failure.kind, "superseded");
    }

    #[test]
    fn completed_single_flight_allows_the_next_refresh() {
        tauri::async_runtime::block_on(async {
            let gate = SingleFlight::default();
            let calls = AtomicUsize::new(0);

            let first = gate
                .run(0, || async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    1
                })
                .await;
            let second = gate
                .run(0, || async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    2
                })
                .await;

            assert_eq!((first, second), (1, 2));
            assert_eq!(calls.load(Ordering::SeqCst), 2);
        });
    }
}
