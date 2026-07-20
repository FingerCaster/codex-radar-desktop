# Backend Error Handling

Errors are classified at the boundary where they become meaningful:

- `SourceError` in `radar/source.rs` represents remote JSON and normalization failures. Its stable `kind()` values are `json`, `schema`, `type`, `timestamp`, and `no_candidates`.
- Private `RadarError` in `radar/service.rs` adds transport/state failures: `network`, `http`, `stale_payload`, `no_cache`, and `response_too_large`.
- `RadarError::as_failure(source)` converts a failure to the serializable
  camel-case `RefreshFailure { source, kind, message, occurredAt }` returned by
  `refresh_radar` and emitted as `radar://refresh-failed` only while that source
  remains active.
- Desktop commands convert Tauri window errors to `String` at the command boundary.

## Last-Known-Good Rule

A failed request, invalid body, oversized response, or older payload must not
replace that source's snapshot or validator. Parsing and same-source stale-time
validation happen before either runtime write. Concurrent same-source callers
clone the same failed `Result`; the single-flight initializer emits and logs it
only once when its captured source is still active. A deselected flight returns
its source-tagged failure to the initiating caller without logging or emitting,
so an old source cannot mark the selected source stale.

Primary and distributed failures never borrow each other's last-known-good
state. A `304` without a cached snapshot for that exact source is `no_cache` and
does not commit the returned validator. `superseded` is an internal service
failure for a command whose `{source, generation}` token is no longer active;
it is never emitted/logged, and the frontend treats it as a no-op so A -> B ->
A cannot let A1 alter A2.

Expected non-critical side effects remain non-fatal. Notification display failure is written to stderr and does not turn a successful refresh into a failure. Event emission currently uses `let _ = app.emit(...)`; it must not change the refresh result.

Do not panic on request data. The only startup assertion is `RadarService::new().expect(...)` in `lib.rs`, because failure to construct the fixed HTTPS client prevents the application service from existing.
