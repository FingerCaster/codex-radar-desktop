# Database Guidelines

The current backend has no database, migration system, repository layer, or persistent Rust cache. Do not introduce database terminology around the existing state.

`RadarServiceInner` in `src-tauri/src/radar/service.rs` stores only process-local state. Each fixed source owns:

```rust
state: RwLock<SourceState>, // matching snapshot + ETag or Last-Modified
refreshes: SingleFlight<Result<RefreshOutcome, RefreshFailure>>,
```

`SourceState` commits its snapshot and validator under one write lock so a
conditional request can never observe a validator paired with another
generation's snapshot. The single-flight cells are temporary coordination
state, not stored data. Both snapshots and validators are lost on restart. The background poll performs an
immediate fetch when the app starts. The frontend's source-specific v2
local-storage entries are presentation fallbacks and are not backend
persistence or HTTP revalidation caches; the source-less v1 entry is read only
for fixed-primary migration.

If durable backend storage is added later, treat it as a new cross-layer/infra
contract: specify the schema, source discriminator, migration and rollback
behavior, corruption handling, retention, and whether a persisted validator may
be paired safely with its exact same-source snapshot. Never persist a validator
alone because an HTTP `304` with no matching snapshot is an explicit `no_cache`
failure.
