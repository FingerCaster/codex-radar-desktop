# Distributed radar source switch - implementation plan

## 1. Domain and source adapters

- [x] Add `RadarSource` and source-tagged snapshot/failure contracts in Rust
  and TypeScript.
- [x] Preserve the primary adapter and add distributed table DTOs, live IQ
  aggregation, stable ranking/ties, timestamp selection, and metric averages.
- [x] Add focused fixtures/tests for schema failures, missing cells, newest-run
  semantics, rounding, ties, ordinary/Ultra cost rules, optional metrics,
  timestamps, and no candidates.

## 2. Source-isolated service

- [x] Replace the single cache/ETag/flight with per-source runtimes and typed
  ETag/Last-Modified validators.
- [x] Apply source-specific endpoints, size limits, and conditional headers;
  preserve last-known-good state on all failures.
- [x] Add generation-token source transitions, per-source latest-generation
  watermarks, atomic cache state, per-caller supersession checks, wake handling,
  and suppress publish/failure/notification side effects from old flights.
- [x] Add service-level pure/state tests for validator selection, per-source
  304 behavior, stale comparisons, activation flight isolation, A-B-A source
  race guards, atomic preference failure, and notification baselines.

## 3. Native menu and persistence

- [x] Extend desktop preferences with the backward-compatible source default.
- [x] Add the native source submenu, exclusive checks, ID mapping, staged
  selection transaction, immediate background refresh, and failure rollback.
- [x] Test legacy JSON migration, source normalization/menu mapping, and that
  source selection does not mutate visibility settings.

## 4. Frontend integration

- [x] Add source-aware type guards, refresh failures, per-source v2 caches, and
  safe primary-only v1 migration.
- [x] Extend the reducer/hook with selected-source transitions and guards for
  stale successes/failures while keeping passive renderers passive.
- [x] Add renderer-session activation epochs plus listener-first/event-wins
  hydration for desktop preferences and main expanded state.
- [x] Open the matching source site through a fixed enum-to-URL allowlist.
- [x] Update unit/runtime/component fixtures and tests for source changes,
  cache isolation, migration, and late-response races.

## 5. Documentation and verification

- [x] Update README source/menu behavior and backend/frontend executable specs.
- [x] Run `pnpm lint`, `pnpm typecheck`, `pnpm test` (9 files / 45 tests), and
  `pnpm build`.
- [x] Run `cargo fmt --check`, `cargo check`, `cargo test` (70 tests), and
  `cargo clippy --all-targets -- -D warnings` from `src-tauri`.
- [x] Run `pnpm tauri build` and record generated Windows MSI/NSIS paths.
  - MSI: `src-tauri/target/release/bundle/msi/Model Radar_0.1.0_x64_en-US.msi`
    (`4,993,024` bytes, SHA-256 `4545D58E3B7D57B9D10E10D88DF3B32ABCA3D8EEA3800A7AC58F0F6E728652C4`)
  - NSIS: `src-tauri/target/release/bundle/nsis/Model Radar_0.1.0_x64-setup.exe`
    (`3,506,732` bytes, SHA-256 `4D72629AB10D83A403E2369F5D3C42AA22D7DCECB160C4C51062A9DF7E0BECC2`)
- [x] Perform a final cross-layer race/data-contract review. The feature shipped
  in `v0.2.0`; the user requested archival after release. Native macOS runtime
  verification remains documented as a platform-specific residual check.

## Risk and rollback points

- Source parser and service state are separate commits/logical checkpoints;
  primary tests must pass before menu wiring proceeds.
- Do not loosen the main response cap or share validators between runtimes.
- Do not modify window geometry, taskbar placement, or visibility code while
  adding the menu.
- Any switch race failure rolls back the source transition code as a unit; the
  existing primary-only path remains the fallback.
