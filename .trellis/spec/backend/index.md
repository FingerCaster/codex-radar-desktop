# Backend Code Specifications

This directory documents the Rust/Tauri backend under `src-tauri/`. The backend owns remote data ingestion, normalized radar state, polling, notifications, tray behavior, and window lifecycle. React receives only normalized camel-case DTOs through Tauri commands and events.

## Documents

| Document | Covers |
| --- | --- |
| [Directory Structure](./directory-structure.md) | Module ownership and placement rules |
| [Desktop Companion Contract](./desktop-companion-contract.md) | Native desktop state, shared tray/taskbar menu, start-at-login, geometry, persistence, and platform routing |
| [Radar Data Contract](./radar-data-contract.md) | Primary/distributed JSON, normalization, source-isolated validators/cache, Tauri events, and errors |
| [Database Guidelines](./database-guidelines.md) | The deliberate no-database design and in-memory state |
| [Error Handling](./error-handling.md) | Typed source/service failures and last-known-good behavior |
| [Logging Guidelines](./logging-guidelines.md) | Current stderr diagnostics and data-safety rules |
| [Quality Guidelines](./quality-guidelines.md) | Rust checks, tests, and change-specific assertions |

Read `radar-data-contract.md` before changing `src-tauri/src/radar/`, `src/types/radar.ts`, Tauri command names, or `radar://*` events. A cross-layer contract change must update that document and both Rust and TypeScript boundary tests in the same change.

Read `desktop-companion-contract.md` before changing `desktop.rs`, native menu
state, window geometry, taskbar/menu-bar behavior, or `desktop://*` events.
