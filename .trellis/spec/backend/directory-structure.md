# Backend Directory Structure

The backend is a single Rust crate rooted at `src-tauri/`.

```text
src-tauri/
|-- Cargo.toml
|-- tauri.conf.json
|-- capabilities/default.json
`-- src/
    |-- main.rs
    |-- lib.rs
    |-- desktop.rs
    |-- desktop/
    |   `-- windows.rs
    `-- radar/
        |-- mod.rs
        |-- domain.rs
        |-- source.rs
        `-- service.rs
```

## Ownership

- `main.rs` is only the native entry point and Windows console setting. Keep application assembly out of it.
- `lib.rs` builds the Tauri application, registers plugins and managed state, installs close-to-hide behavior, and lists commands.
- `desktop.rs` owns window sizing/positioning, hide behavior, and tray actions. It must not parse radar data or perform HTTP requests.
- `desktop/windows.rs` owns Windows 11 taskbar discovery, physical child
  placement, and pure Win32 geometry tests. It must remain behind
  `cfg(windows)` and must not own preferences or radar state.
- `radar/domain.rs` owns app-controlled serializable DTOs (`RadarSource`,
  `ModelScore`, `Attribution`, and `RadarSnapshot`).
- `radar/source.rs` is the only remote-schema adapter for both fixed sources.
  Partial primary/distributed serde structs stay here; callers receive a
  source-tagged `RadarSnapshot`, never upstream DTOs.
- `radar/service.rs` owns the HTTPS client, active source, per-source
  ETag/Last-Modified validator, in-memory last-known-good snapshots,
  single-flights, polling, commands/events, and leader notifications.
- `radar/mod.rs` exposes only the items required by application assembly.

Keep platform behavior in `desktop`, wire-format normalization in `source`, and state/I/O orchestration in `service`. This separation lets a remote schema change remain isolated from React and desktop lifecycle code.
