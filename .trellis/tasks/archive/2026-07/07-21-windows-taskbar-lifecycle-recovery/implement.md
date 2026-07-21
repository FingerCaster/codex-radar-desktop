# Implement: Windows taskbar lifecycle and recovery

## Checklist

1. `desktop/windows.rs`: replace global hook boolean/mutex callback path with a
   lazily started dedicated message-loop worker, bounded command/ack protocol,
   non-blocking action post, atomic hit rect/event sequence, 30-second lease +
   cursor heartbeat rearm, serialized lifecycle gate, disable-time worker
   termination, explicit shutdown, and child-owned visibility-style helper.
2. `desktop/windows.rs` + `desktop.rs`: implement a tokenized `Ready/Recovering/Fatal` taskbar lifecycle drive; destroy detached canonical labels once, wait for Manager removal, tolerate missing/changing Explorer host for 10 seconds, and never close/build the same label synchronously.
3. `desktop.rs`: expose main while recovering, restore taskbar-only state only after a healthy rebuild, and demote on timeout/deterministic failure.
4. `desktop.rs`: introduce one preference transition gate for every full-preference writer and refactor commit/rollback so the preference value mutex is released before Wry/Win32 operations.
5. `desktop.rs`: make tray toggle native-visibility-aware and reorder permanent failure recovery/commit while preserving aggregated warnings and unrelated preference fields.
6. Add unit tests for hook action/rect/lease decisions, rebuild plan/tokens/timeout, close policy, main-hide guard ordering and native/preference drift.
7. Update `.trellis/spec/backend/desktop-companion-contract.md` and `.trellis/spec/backend/quality-guidelines.md` with the executable lifecycle contracts.
8. Bound the `TaskbarView` root to the actual client viewport, rebalance its
   two CSS grids for a shrinkable model/score, and add a long-content layout
   regression test plus frontend validation.
9. Resolve taskbar physical size with Tauri scale plus the child HWND DPI lower
   bound, retain full-size-or-fail geometry, and test override/fallback cases.

## Validation

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build
```

Manual Windows smoke checks, without restarting Explorer:

1. Start with taskbar projection disabled, enable it from the native menu, and verify left/right companion actions.
2. Disable taskbar projection and verify tray/main recovery remains functional; re-enable and verify a fresh hook thread works.
3. Hide main while taskbar is healthy, then use taskbar and tray left-click to show it on the first click.
4. Keep the app running across at least one hook rotation interval and repeat left/right clicks.

## Verification Result

- `cargo fmt --check`, full-target Clippy with `-D warnings`, and all 100 Rust
  tests passed.
- Frontend lint, typecheck, all 83 Vitest tests, and production build passed.
- `pnpm tauri build` produced the release executable plus MSI and NSIS bundles.
- Interactive testing of the new executable was not run because the installed
  single-instance build was still active. That user process and Explorer were
  deliberately left running; native hook/rebuild smoke remains a release check.

## Risk And Rollback Points

- Raw message structs and FFI signatures in `windows.rs` must match Win32 ABI; compile, clippy and live smoke checks are mandatory.
- Never join the hook thread from its own callback/message loop.
- A rebuilding label or temporarily absent Explorer host must not be treated as fatal before the 10-second grace deadline, but the deadline must prevent infinite no-surface recovery.
- Do not cache `Shell_TrayWnd`; Explorer handle replacement is part of the recovery contract.
- Do not hold the preference value mutex across any native call introduced by this task.
