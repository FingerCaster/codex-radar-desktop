# Backend Logging Guidelines

There is no logging framework, telemetry, analytics, or persisted log file. Runtime diagnostics use stderr with the stable `[model-radar]` prefix in `src-tauri/src/radar/service.rs`:

```rust
eprintln!("[model-radar] {error}");
eprintln!("[model-radar] notification unavailable: {error}");
```

Use this mechanism only for actionable backend failures that otherwise have no operator-visible diagnostic. Refresh failures are also sent to the UI as `RefreshFailure`; logging must not replace that event.

Never log response bodies, remote headers, ETags, full normalized snapshots, or future secrets/tokens. The current source is public, but retaining raw upstream data would add an unnecessary data surface. Keep messages single-line where the underlying error permits it and include the operation context (`network request`, `notification`) rather than an ambiguous debug dump.
