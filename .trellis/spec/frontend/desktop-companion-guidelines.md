# Desktop Companion Renderer Guidelines

## 1. Scope / Trigger

This contract applies when changing `App`, `TaskbarView`, desktop preference
types/adapters/hooks, taskbar CSS, drag regions, or radar hook behavior shared
by the `main` and `taskbar` renderers.

## 2. Signatures

Renderer boundaries are owned by `src/lib/desktop.ts`:

```text
getDesktopPreferences() -> DesktopPreferences
setDesktopOption(option, enabled) -> DesktopPreferences
setDesktopOpacity(opacityPercent) -> DesktopPreferences
getMainExpanded() -> boolean
showMainDetails() -> void
showDesktopContextMenu() -> void
updateCompanionProjection(projection) -> void
onDesktopPreferencesUpdated(handler) -> UnlistenFn
onMainExpanded(handler) -> UnlistenFn
```

`useRadar({ passive: true, source, activationEpoch, enabled })` is the taskbar
mode. `useRadar({ source, activationEpoch, enabled })` is the active
main-renderer mode. `source`, `activationEpoch`, and `enabled` come from
`useDesktopPreferences`; both renderers wait for the complete native preference
before radar hydration.

## 3. Contracts

- `App` branches on the current WebView label. `main` renders compact/detail;
  `taskbar` renders only `TaskbarView`.
- The taskbar renderer hydrates the Rust snapshot and listens to snapshot and
  failure events, but it does not call refresh, request notification
  permission, listen for refresh requests, or register online recovery.
- Components consume normalized `RadarSnapshot` and semantic callbacks. They
  never parse upstream JSON or call Tauri `invoke` directly.
- Validate every desktop preference payload at the IPC/event boundary before
  applying drag regions, opacity, visibility projection, or layout.
- Register the desktop-preference event listener before calling
  `getDesktopPreferences`. If an event arrives while the initial request is in
  flight, the event wins and the delayed initial success/failure is ignored.
- `radarActivationEpoch` is renderer-session state, not a persisted preference.
  Increment it for every actual `radarSource` transition, including both legs
  of main -> distributed -> main.
- The complete preference guard requires `radarSource` to be exactly `main` or
  `distributed`. A source change updates both renderers, but only Rust starts
  the immediate refresh; the passive renderer never becomes an active caller.
- The Windows taskbar surface is exactly `168 x 30` CSS pixels. Row one shows
  model/effort/status and row two shows IQ/value/ties. Row heights plus vertical
  padding total exactly 30px, and the surface stays transparent and borderless.
- `.taskbar-effort` uses `10px`, weight `700`, and the existing `12px` line
  height inside its fixed `34px` track. Longer effort values ellipsize; the
  stronger `max` label must not change either row or native viewport geometry.
- Long model, effort, score, tie, and status text must ellipsize inside fixed
  tracks. No taskbar content may resize the native child.
- `positionLocked` removes every app-provided `data-tauri-drag-region`; it does
  not disable operating-system window movement shortcuts or the five native
  quick-position presets.
- Opacity is projected from the validated 60/70/80/90/100 value through one
  CSS custom property on the renderer root.
- Main expanded state hydrates through `getMainExpanded` after listener
  registration so a companion click cannot be lost during renderer startup.
  If `onMainExpanded` fires while the initial read is pending, that event wins;
  ignore both a delayed initial value and a delayed initial error.

## 4. Validation & Error Matrix

| Input/state | Renderer behavior |
|---|---|
| Malformed desktop preference event | Ignore it; retain last valid state |
| Preference not hydrated | Show bounded booting projection; read no default-main radar cache |
| Source or activation epoch changes | Mask old activation immediately, then hydrate only the target source cache |
| Taskbar starts with no Rust snapshot | Show bounded unavailable/loading projection; do not refresh |
| Snapshot event is valid | Update projection and cache through the radar reducer |
| Fixed taskbar projection | Show model/effort/status above IQ/value/ties within 168px |
| Position locked | Render no drag-region attributes |
| Native action rejects | Keep current view/state and expose the existing recovery path |

## 5. Good / Base / Bad Cases

- Good: one active main renderer and one passive taskbar renderer receive the
  same Rust snapshot while only the main renderer can initiate refresh.
- Good: native source selection changes both projections to distributed without
  an intermediate main snapshot or a second polling loop.
- Good: a tied leader renders `+N` without changing taskbar dimensions and the
  accessible name includes the full tie count.
- Base: no snapshot renders `暂无数据`, `--`, and a bounded status label.
- Bad: a second `useRadar()` active instance would duplicate refresh-request,
  notification, and online side effects even with backend single-flight.
- Bad: styling the 30px taskbar surface with a border or opaque card makes it
  look detached from the system taskbar and consumes layout pixels.
- Bad: adding an optional wider mode can cover task buttons or another embedded
  monitor and is not part of the desktop preference contract.

## 6. Tests Required

- Desktop boundary tests accept the complete camel-case preference object and
  reject missing, mistyped, unsupported opacity, and unknown source values.
- Preference-hook tests prove listener-before-read ordering, event-wins
  hydration, late registration cleanup, same-source epoch stability, and
  main -> distributed -> main epoch accumulation.
- Projection tests cover primary leader, ties, stale status, and no snapshot.
- Hook runtime tests prove passive mode registers snapshot/failure listeners
  but not refresh-request, notification permission, online recovery, or an
  initial refresh; they also prove disabled hydration reads no cache and a
  source change synchronously masks the old projection.
- App runtime tests prove an expanded event received during hydration is not
  overwritten by a delayed `getMainExpanded` success or failure.
- Taskbar component tests assert details click, context-menu callback, effort,
  freshness, tie marker, accessible name, and that the polite status live
  region is outside the primary button.
- Window-view tests assert all drag markers disappear when locked.
- Run frontend lint, typecheck, tests, and production build; run Rust checks
  whenever command/event or native geometry behavior changes.

## 7. Wrong vs Correct

Wrong:

```tsx
const radar = useRadar({ source: "main" }); // hard-coded before preferences hydrate
```

Correct:

```tsx
const isTaskbar = getCurrentWebviewLabel() === "taskbar";
const desktop = useDesktopPreferences();
const radar = useRadar({
  passive: isTaskbar,
  source: desktop.preferences.radarSource,
  activationEpoch: desktop.radarActivationEpoch,
  enabled: desktop.hydrated,
});
```

The passive projection remains live without becoming a second side-effect
owner.
