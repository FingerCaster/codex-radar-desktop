# Frontend Development Guidelines

> Executable conventions for the Tauri 2 + React 19 + TypeScript renderers in Model Radar.

## Scope

The frontend renders compact/detail `main` projections and a passive Windows
`taskbar` projection of Rust-normalized radar state. Rust owns primary and
distributed parsing, ranking, source-specific conditional refresh, polling,
notifications, desktop preferences,
tray actions, and native window lifecycle. React owns presentation,
source-specific last-rendered cache hydration, and live command/event synchronization.

Read [the backend radar data contract](../backend/radar-data-contract.md) for the normalized cross-layer payload and Rust failure behavior. Frontend code must not parse either upstream JSON shape.

## Guidelines Index

| Guide | Concrete coverage |
|---|---|
| [Directory Structure](./directory-structure.md) | Actual `src/` tree, dependency direction, module ownership, naming |
| [Component Guidelines](./component-guidelines.md) | `App`, compact/detail views, exact props, actions, accessibility |
| [Hook Guidelines](./hook-guidelines.md) | `useRadar`, command/event lifecycle, listener cleanup, cache effects |
| [State Management](./state-management.md) | Reducer action union, complete status transition matrix, last-known-good invariant |
| [Type Safety](./type-safety.md) | Snapshot/failure types, IPC signatures, cache key, runtime guard, validation/error matrix |
| [Quality Guidelines](./quality-guidelines.md) | Commands, test conventions, CSS fixed dimensions, high-DPI and review checks |
| [Desktop Companion Guidelines](./desktop-companion-guidelines.md) | Main/taskbar renderer split, passive radar mode, desktop IPC, fixed companion layout |

All seven guides describe the current implementation and are active.

## Recommended Reading Order

1. Start with [Directory Structure](./directory-structure.md) before placing code.
2. Read [Type Safety](./type-safety.md) before changing IPC, events, cache fields, or snapshot types.
3. Read [Hook Guidelines](./hook-guidelines.md) and [State Management](./state-management.md) before changing refresh behavior.
4. Read [Desktop Companion Guidelines](./desktop-companion-guidelines.md)
   before changing desktop commands, renderer routing, or taskbar layout.
5. Read [Component Guidelines](./component-guidelines.md) and [Quality Guidelines](./quality-guidelines.md) before changing layout or controls.

## Core Frontend Invariants

- Components receive normalized `RadarSnapshot` props and semantic actions; they never call remote endpoints or parse raw source data.
- `src/lib/radar.ts` owns the radar/cache boundary and `src/lib/desktop.ts`
  owns the desktop command/event boundary.
- `useRadar` is the only shared radar hook and `radarReducer` is the only radar
  state transition mechanism; the taskbar renderer always uses passive mode.
- Radar hydration waits for validated native `radarSource`; cache keys,
  snapshots, failures, and reducer actions remain source-tagged end to end.
- Refresh failure and refresh-in-progress preserve the same-source last-known-good snapshot.
- Compact and expanded views share the main native window; the Windows taskbar
  projection uses a separate fixed `168 x 30` child WebView.
- The detail view always preserves source attribution, and the browser opener
  maps the selected enum to one of two fixed allowlisted source sites.
- Local storage and snapshot events are accepted only through `isRadarSnapshot`.
- Icon-only controls use Lucide through `IconButton` with accessible labels and tooltips.

## Validation Baseline

```powershell
pnpm lint
pnpm typecheck
pnpm test
pnpm build
```

Run the Rust/Tauri validation listed in the task and backend specs whenever a frontend change alters commands, events, window geometry, notification permission, or the normalized snapshot contract.
