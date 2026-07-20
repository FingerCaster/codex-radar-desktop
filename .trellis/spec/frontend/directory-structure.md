# Frontend Directory Structure

> React presents Rust-owned state in the main window and a passive Windows taskbar WebView. Rust owns remote data, polling, notifications, tray behavior, desktop preferences, and window lifecycle.

## Current Layout

```text
src/
|-- main.tsx                    # React root and StrictMode
|-- App.tsx                     # View-mode composition and window actions
|-- App.css                     # Global tokens and both fixed window layouts
|-- components/
|   |-- CompactView.tsx         # 360 x 112 summary surface
|   |-- DetailView.tsx          # Expanded ranking and attribution surface
|   |-- TaskbarView.tsx         # 168 x 30 passive two-row projection
|   `-- IconButton.tsx          # Accessible Lucide icon control
|-- hooks/
|   |-- useDesktopPreferences.ts # Validated desktop state hydration/events
|   |-- useRadar.ts             # Source-aware reducer plus Tauri/cache synchronization
|   `-- useRadar.test.ts        # Reducer transition tests
|-- lib/
|   |-- desktop.ts              # Desktop IPC/events and companion projection
|   |-- model.ts                # Pure model display-name formatting
|   |-- model.test.ts
|   |-- radar.ts                # IPC, source-specific cache/event guards, fixed opener
|   `-- radar.test.ts
|-- test/
|   `-- fixtures.ts             # Shared normalized RadarSnapshot fixture
|-- types/
|   |-- desktop.ts              # Desktop preferences/projection contracts
|   `-- radar.ts                # Renderer-owned domain and component prop types
`-- vite-env.d.ts

public/
`-- app-icon.svg                # Favicon referenced by index.html
```

The generated `dist/` directory and the Tauri `src-tauri/target/` directory are build output, not source. The remaining starter assets under `public/` and `src/assets/` are not implementation examples because the radar UI does not import them.

## Module Boundaries

The dependency direction is deliberate:

```text
main.tsx -> App.tsx -> components/*
                    -> hooks/useRadar.ts -> lib/radar.ts -> Tauri APIs
                    -> lib/radar.ts -> Tauri APIs
components/* -> lib/model.ts
components/* -> types/radar.ts
tests -> test/fixtures.ts
```

- `src/types/radar.ts` contains stable data shapes and view prop contracts. It must not import React components, hooks, or Tauri APIs.
- `src/lib/radar.ts` owns radar Tauri/cache/notification/opener boundaries;
  `src/lib/desktop.ts` owns desktop command/event boundaries. Components must
  not call `invoke` or `listen` directly.
- `src/hooks/useRadar.ts` owns asynchronous synchronization and exposes a renderer-ready state projection. It does not format user-facing strings.
- `src/App.tsx` routes by WebView label, composes main compact/detail or
  TaskbarView, and owns transient window-mode errors. It does not parse
  snapshots.
- `src/components/` contains presentational views. Formatting helpers local to one view remain in that view; reusable pure model formatting lives in `src/lib/model.ts`.
- `src/App.css` is global because compact and expanded layouts share tokens and structural classes. This project does not use CSS Modules, CSS-in-JS, or utility classes.

The normalized Rust/TypeScript bridge contract is documented in [the backend radar data contract](../backend/radar-data-contract.md). Do not duplicate upstream JSON field knowledge in renderer modules.

## Adding Frontend Code

Place code by ownership rather than file size:

| Need | Location | Existing example |
|---|---|---|
| New radar presentation | `src/components/` | `DetailView.tsx` |
| Stateful renderer orchestration | `src/hooks/` | `useRadar.ts` |
| Tauri IPC/event/cache boundary | `src/lib/radar.ts` | `refreshRadar`, `onSnapshotUpdated` |
| Pure reusable formatting | `src/lib/` | `getModelDisplayName` |
| Shared renderer types/props | `src/types/` | `RadarSnapshot`, `CompactViewProps` |
| Shared typed test data | `src/test/` | `sampleSnapshot` |

Do not create a feature-level global store for this MVP. Each renderer has one
`useRadar` reducer instance; only the main instance is active and the taskbar
instance is passive.

## Naming And Imports

- React component files and exported component functions use PascalCase: `CompactView.tsx`, `CompactView`.
- Hooks use a `use` prefix and camelCase: `useRadar.ts`, `useRadar`.
- Pure library and type modules use lower camel-case filenames: `model.ts`, `radar.ts`.
- Tests are colocated with the module and use `*.test.ts` or `*.test.tsx`.
- Shared types are imported with `import type` when no runtime value is needed.
- Internal imports are relative. There is no configured path alias.
- Named exports are the implementation contract. Components currently also provide default exports, but callers use named imports.

## Wrong vs Correct

### Wrong

```tsx
// A component now owns transport, raw payload shape, and presentation.
const raw = await invoke<unknown>("refresh_radar");
const score = (raw as any).model_iq.latest.iq;
```

### Correct

```tsx
// App/components consume renderer-owned normalized state.
const radar = useRadar();
return <CompactView snapshot={radar.snapshot} status={radar.status} {...actions} />;
```

The first form bypasses both the Rust source adapter and `isRadarSnapshot`; it also couples a view to the upstream schema.
