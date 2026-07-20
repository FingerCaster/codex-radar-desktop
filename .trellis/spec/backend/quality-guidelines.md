# Backend Quality Guidelines

Run the crate checks from the repository root after changing Rust code:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

For command/event or DTO changes, also run `pnpm typecheck`, `pnpm test`, and `pnpm build`; Rust serde output and `src/types/radar.ts` are one contract.

## Current Test Placement

- `radar/source.rs` tests primary compatibility plus distributed latest-run IQ,
  stable ties, ordinary/Ultra cost rules, duration/timestamp aggregation,
  malformed JSON, schema/type/timestamp rejection, and empty candidates.
- `radar/service.rs` tests per-source endpoint/header/limit/interval policy,
  atomic snapshot/validator/304 state, stale/leader isolation, per-source latest
  generation watermarks, inactive publication guards, independent generation
  flights, late joined callers, shared same-source failure, and next-flight
  cleanup.
- `desktop.rs` tests preference recovery, platform click routing, fitting
  logical dimensions, and reversible main-window placement across scaled and
  negative-origin work areas.
- `desktop.rs` position tests cover five work-area presets, compact-equivalent
  expanded capture, malformed/negative JSON, disconnected and mixed-DPI monitor
  recovery, singleton writer startup, and writer-error lost-wakeup recovery.
- `desktop/windows.rs` tests exact non-shrinking taskbar geometry at
  100/125/150/200 percent, rejects a companion that cannot fit, detects an
  unchanged child rectangle, and moves left when a blocker appears at runtime.
- `desktop.rs` tests singleton monitor claiming and the safe taskbar-to-main
  preference demotion used after runtime placement failure.
- Frontend boundary tests validate cached DTOs and prevent older async snapshots from replacing current data.

Keep deterministic assertions on stable identities and order, not only collection lengths. Any change to HTTP revalidation or refresh concurrency needs a service-level test for each source's request header, `304` behavior, cached state, independent/collapsed flight behavior, inactive publication guard, and exactly-once event/notification decisions; HTTP and AppHandle paths do not currently have an integration harness. Any new serializable field needs a Rust serialization assertion and a TypeScript validator fixture.

When reviewing background desktop work, inspect Tauri/Wry thread affinity in
addition to ordinary mutex order. Off-main-thread window getters synchronously
wait for the event loop, so they must not run while holding a lock that a tray,
menu, close, or exit callback can synchronously acquire on the main thread.
