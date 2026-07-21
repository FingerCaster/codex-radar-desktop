# Implement: desktop recovery harden

1. `windows.rs`: `taskbar_companion_is_healthy` (attached + visible).
2. `desktop.rs` `apply_visibility`: taskbar first; only then hide main.
3. `desktop.rs` monitor: recreate missing companion once; demote on unhealthy/failed place.
4. `desktop.rs` recovery: clamp main to work area + show (shared helper for demotion + show-main).
5. Tests + `desktop-companion-contract.md` update.
6. `cargo test` / clippy on `src-tauri`.
