# Desktop Companion Renderer Guidelines

## 1. Scope / Trigger

This contract applies when changing `App`, `SettingsView`, `TaskbarView`,
desktop preference types/adapters/hooks, taskbar/settings CSS, drag regions,
or radar hook behavior shared by the `main` and `taskbar` renderers.

## 2. Signatures

Renderer boundaries are owned by `src/lib/desktop.ts`:

```text
getDesktopPreferences() -> DesktopPreferences
setDesktopOption(option, enabled) -> DesktopPreferences
setDesktopOpacity(opacityPercent) -> DesktopPreferences
setDesktopRadarSource(source) -> DesktopPreferences
setMainWindowPositionPreset(
  preset: "top-left" | "top-right" | "center" | "bottom-left" | "bottom-right"
) -> void
getMainExpanded() -> boolean
showMainDetails() -> void
showDesktopContextMenu() -> void
updateCompanionProjection(projection) -> void
onDesktopPreferencesUpdated(handler) -> UnlistenFn
onMainExpanded(handler) -> UnlistenFn
onShowMainDetails(handler) -> UnlistenFn

resolveModelMarkKind(model?, displayName?) -> "codex" | "sol" | "terra" | "luna"
```

`useRadar({ passive: true, source, activationEpoch, enabled })` is the taskbar
mode. `useRadar({ source, activationEpoch, enabled })` is the active
main-renderer mode. `source`, `activationEpoch`, and `enabled` come from
`useDesktopPreferences`; both renderers wait for the complete native preference
before radar hydration.

## 3. Contracts

- `App` branches on the current WebView label. `main` owns an explicit
  `compact | detail | settings` view state; `taskbar` renders only
  `TaskbarView`. Settings uses the existing expanded native geometry and never
  creates another WebView.
- Opening settings from compact expands natively before mounting it and stores
  compact as the return view. Opening from detail performs no resize and
  returns to detail. Back/Escape update React only after any required native
  shrink succeeds; failure leaves settings mounted in coherent expanded
  geometry.
- `SettingsView` is presentational. It receives the complete accepted
  `DesktopPreferences`, semantic option/opacity/source/position callbacks, one
  pending discriminator, and a bounded error. It never invokes Tauri,
  calculates monitor coordinates, or optimistically mutates a preference.
- Settings renders `快捷位置` as exactly five fixed icon controls in native-menu
  order: top-left, top-right, center, bottom-left, bottom-right. Every control
  has a Chinese accessible name and hover title. The callback sends only the
  shared preset union through `src/lib/desktop.ts`; success keeps settings
  mounted and does not change `DesktopPreferences`.
- The taskbar renderer hydrates the Rust snapshot and listens to snapshot and
  failure events, but it does not call refresh, request notification
  permission, listen for refresh requests, or register online recovery.
- Components consume normalized `RadarSnapshot` and semantic callbacks. They
  never parse upstream JSON or call Tauri `invoke` directly.
- Validate every desktop preference payload at the IPC/event boundary before
  applying drag regions, opacity, visibility projection, or layout.
- The complete preference guard requires `launchAtLogin` to be a boolean.
  Renderer code treats it like every other native preference; Rust owns OS
  registration, reconciliation, persistence, and rollback.
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
  The root also uses `max-width: 100%` and `max-height: 100%`: native child
  embedding and DPI conversion can expose a client viewport narrower than the
  nominal size, and the renderer must shrink inside it instead of clipping its
  rightmost status beneath the next taskbar component.
- The first taskbar row uses `10px minmax(0, 1fr) 34px 30px` with a `2px`
  gap and `3px` horizontal surface padding. Only the model track absorbs width;
  the ordinary three-character status and effort remain visible. The second
  row uses `17px minmax(0, 1fr) minmax(0, max-content)`, so a long IQ value
  ellipsizes before a present tie marker is displaced.
- When the embedded CSS viewport is below 30px high, remove vertical surface
  padding so the fixed `12px + 14px` rows still fit down to 26px. This is a
  renderer fallback for transient/native mismatch, not permission to shrink
  the requested native viewport.
- `.taskbar-effort` uses `10px`, weight `700`, and the existing `12px` line
  height inside its fixed `34px` track. Longer effort values ellipsize; the
  stronger `max` label must not change either row or native viewport geometry.
- Long model, effort, score, tie, and status text must ellipsize inside fixed
  tracks. No taskbar content may resize the native child.
- Model artwork is bundled locally and decorative (`alt=""`,
  `aria-hidden="true"`). Exact stable identifiers `gpt-5.6-sol`,
  `gpt-5.6-terra`, and `gpt-5.6-luna` resolve to the sun, earth, and moon PNGs.
  GPT-5.5, GPT-5.4, every unknown stable identifier, and every other model use
  the Codex SVG. Only when a stable identifier is absent may the taskbar
  display name infer a bounded `GPT-5.6 Sol|Terra|Luna` family token.
- `positionLocked` removes every app-provided `data-tauri-drag-region`; it does
  not disable operating-system window movement shortcuts or the five native
  quick-position presets.
- Opacity is projected from the validated 60/70/80/90/100 value through one
  CSS custom property on the renderer root.
- Main expanded state hydrates through `getMainExpanded` after listener
  registration so a companion click cannot be lost during renderer startup.
  If `onMainExpanded` fires while the initial read is pending, that event wins;
  ignore both a delayed initial value and a delayed initial error.
- Start `onMainExpanded` and `onShowMainDetails` registration concurrently,
  retain each successful unlistener independently, and call
  `getMainExpanded` only after both registration promises settle. A native
  `desktop://show-main-details` intent explicitly leaves settings for detail;
  ordinary expanded geometry events preserve settings. Cleanup must also
  dispose a listener whose promise resolves after unmount.

## 4. Validation & Error Matrix

| Input/state | Renderer behavior |
|---|---|
| Malformed desktop preference event | Ignore it; retain last valid state |
| Preference not hydrated | Show bounded booting projection; read no default-main radar cache |
| Source or activation epoch changes | Mask old activation immediately, then hydrate only the target source cache |
| Taskbar starts with no Rust snapshot | Show bounded unavailable/loading projection; do not refresh |
| Snapshot event is valid | Update projection and cache through the radar reducer |
| Fixed taskbar projection | Show model/effort/status above IQ/value/ties within 168px |
| Embedded client width is below 168 CSS pixels | Cap the root to the parent, preserve fixed metadata tracks, and ellipsize only shrinkable model/score content |
| Known Sol/Terra/Luna stable identifier | Resolve its matching local PNG without a network request |
| GPT-5.5, GPT-5.4, or unknown stable identifier | Resolve the Codex SVG even if display text contains a different family token |
| Position locked | Render no drag-region attributes |
| Position locked with an explicit preset | Keep all five preset controls enabled unless another settings command is pending |
| Position preset succeeds | Keep settings mounted and clear pending state without changing a preference value |
| Native action rejects | Keep current view/state and expose the existing recovery path |
| Setting command pending | Disable settings/back controls, suppress duplicate updates, and show one bounded saving status |
| Setting command rejects | Keep event-owned values selected, clear pending state, and show a bounded alert |
| Compact return resize rejects | Keep settings mounted in expanded geometry and expose the window error |
| Show-details intent arrives while listener registration is in flight | The concurrently started intent listener receives it and selects detail; a delayed geometry registration is cleaned up safely |

## 5. Good / Base / Bad Cases

- Good: one active main renderer and one passive taskbar renderer receive the
  same Rust snapshot while only the main renderer can initiate refresh.
- Good: native source selection changes both projections to distributed without
  an intermediate main snapshot or a second polling loop.
- Good: a tied leader renders `+N` without changing taskbar dimensions and the
  accessible name includes the full tie count.
- Good: a high-DPI embedded client narrower than 168 CSS pixels keeps `max`,
  `已同步`, IQ, and a present tie marker inside the surface while the model and
  long score ellipsize.
- Good: `gpt-5.6-terra` renders the local earth artwork beside its existing
  accessible model text, while `gpt-5.5-codex-max` renders the Codex mark.
- Good: start-at-login toggled in settings remains pending until Rust verifies
  the OS registration, then the complete preference event selects the new
  value and the shared native menu check matches it.
- Good: `移到下右` remains available while drag locking is enabled and routes
  `bottom-right` to Rust without deriving screen or taskbar coordinates.
- Base: no snapshot renders `暂无数据`, `--`, and a bounded status label.
- Bad: a second `useRadar()` active instance would duplicate refresh-request,
  notification, and online side effects even with backend single-flight.
- Bad: styling the 30px taskbar surface with a border or opaque card makes it
  look detached from the system taskbar and consumes layout pixels.
- Bad: adding an optional wider mode can cover task buttons or another embedded
  monitor and is not part of the desktop preference contract.
- Bad: keeping an unconditional 168px child root when its parent client area is
  narrower hides the fixed status track under the adjacent taskbar component.
- Bad: computing preset coordinates from `window.screen` in React can select the
  wrong monitor work area and bypass native compact-position persistence.
- Bad: substring matching `sol`, `terra`, or `luna` inside every model string
  misclassifies names such as `gpt-5.6-solaris` and overrides stable IDs.
- Bad: sequentially awaiting the expanded listener before starting the details
  listener can strand a visible settings page after a taskbar details click.

## 6. Tests Required

- Desktop boundary tests accept the complete camel-case preference object and
  reject missing/mistyped `launchAtLogin`, unsupported opacity, and unknown
  source values.
- Preference-hook tests prove listener-before-read ordering, event-wins
  hydration, late registration cleanup, same-source epoch stability, and
  main -> distributed -> main epoch accumulation.
- Projection tests cover primary leader, ties, stale status, and no snapshot.
- Model-mark tests cover exact Sol/Terra/Luna identifiers, explicit GPT-5.5 and
  GPT-5.4 fallback, unknown identifiers, bounded display-name fallback, and
  decorative image semantics in compact/detail/ranking/taskbar projections.
- Hook runtime tests prove passive mode registers snapshot/failure listeners
  but not refresh-request, notification permission, online recovery, or an
  initial refresh; they also prove disabled hydration reads no cache and a
  source change synchronously masks the old projection.
- App runtime tests prove an expanded event received during hydration is not
  overwritten by a delayed `getMainExpanded` success or failure; details
  intent registration starts even while expanded registration is unresolved;
  late registrations are cleaned up; settings restores compact/detail; and a
  failed shrink keeps settings mounted.
- Settings component tests assert accessible checkbox/segmented/back controls,
  exact semantic values, the five ordered position commands, locked-state
  availability, global pending disablement, and bounded errors without
  optimistic selection.
- Desktop adapter and App tests assert the exact kebab-case preset payload,
  shared `positionPreset` pending state, success cleanup, and rejection cleanup.
- Taskbar component tests assert details click, context-menu callback, effort,
  freshness, tie marker, accessible name, and that the polite status live
  region is outside the primary button.
- Taskbar layout tests load the production stylesheet and assert the bounded
  root plus exact shrinkable/fixed grid tracks using long model, effort, status,
  score, and tie values; full content remains available through titles and the
  accessible button name.
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

Wrong:

```tsx
const expandedUnlisten = await onMainExpanded(handleExpanded);
const detailsUnlisten = await onShowMainDetails(handleDetails);
```

Correct:

```tsx
const registrations = await Promise.allSettled([
  onMainExpanded(handleExpanded),
  onShowMainDetails(handleDetails),
]);
```

Start both listener registrations before awaiting either one, then retain and
clean up each fulfilled unlistener independently before reading initial state.

Wrong:

```tsx
onClick={() => invoke("set_main_window_position", getScreenCoordinates())}
```

Correct:

```tsx
onClick={() => onSetPositionPreset("bottom-right")}
```

`SettingsView` emits a semantic action; the typed desktop adapter and Rust
controller own IPC, monitor geometry, rollback, and persistence.

Wrong:

```typescript
if (model.includes("sol")) return solLogo;
```

Correct:

```typescript
return MODEL_MARK_BY_IDENTIFIER[model.trim().toLowerCase()] ?? "codex";
```

Stable model identifiers take priority. A tightly bounded display-name fallback
exists only for the taskbar projection when no identifier is available.

Wrong:

```css
.taskbar-view { width: 168px; }
.taskbar-primary-row { grid-template-columns: 10px 1fr 34px 44px; }
```

Correct:

```css
.taskbar-view { width: 168px; max-width: 100%; }
.taskbar-primary-row {
  grid-template-columns: 10px minmax(0, 1fr) 34px 30px;
}
```

The nominal native contract stays fixed while the renderer remains bounded by
the actual embedded client area.
