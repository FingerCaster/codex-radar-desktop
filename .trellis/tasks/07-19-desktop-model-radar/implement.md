# Desktop Model Radar MVP - Implementation Plan

## Ordered Checklist

- [x] Scaffold pnpm + React/TypeScript/Vite + Tauri 2 with project-local CLI.
- [x] Add lint, typecheck, frontend test, Rust test, and build dependencies.
- [x] Implement typed Rust public-summary adapter, ranking, tie handling, and
  stale timestamp validation with fixtures and unit tests.
- [x] Implement shared service state, ETag conditional refresh, five-minute
  background polling, event emission, and leader-set notification dedupe.
- [x] Implement one-window resize/hide lifecycle and tray menu behavior.
- [x] Implement compact and expanded React views with live events, manual
  refresh, cached last-known-good data, offline/stale handling, and attribution.
- [x] Add app/tray icons and minimum Tauri capabilities.
- [x] Run formatting, linting, type checking, tests, and frontend production
  build; fix all failures.
- [x] Launch the desktop app and visually inspect compact, expanded, loading,
  and stale states with accessibility-tree and screenshots.
- [x] Produce a Windows Tauri bundle and verify expected artifacts.
- [x] Replace placeholder Trellis frontend/backend guidelines with conventions
  evidenced by the implemented code, then run the final Trellis quality pass.

## Validation Commands

```powershell
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm exec tauri info
pnpm tauri build
```

## Review Gates

- Before UI work: source fixtures prove normalization and exact tie behavior.
- Before notification work: leader-set identity comparison is unit tested and
  first-fetch behavior is explicitly separated from later refreshes.
- Before bundle build: no frontend code parses remote payload fields directly,
  no arbitrary URL/shell permission exists, and attribution is visible.
- Before completion: inspect both window sizes and force a failed refresh to
  confirm last-known-good behavior.

## Risk And Rollback Points

- Remote terms: do not distribute or call the full API without source-owner
  authorization. Keep attribution visible in all detail builds.
- Remote schema: fail closed and retain cached data; never synthesize a leader
  from malformed fields.
- Platform UI: keep the window opaque and use core APIs. Defer any platform-
  specific appearance feature that breaks another desktop target.
- Native notification: lack of permission is non-fatal and must not block state
  updates or polling.

## Completion Note

The MVP and its six follow-up children shipped through the public `v0.2.0`
release. Child tasks intentionally evolved the original platform and feature
scope; their archived PRDs and executable specs are authoritative for the
final Windows/macOS support boundary.
