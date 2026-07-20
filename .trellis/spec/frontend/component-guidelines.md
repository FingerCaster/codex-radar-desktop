# Component Guidelines

> Components render normalized radar state and emit semantic actions. They do not fetch, cache, or parse data.

## Component Roles

`src/App.tsx` is the composition root for both renderer labels. The `main`
label uses the compact/detail composition below; the `taskbar` label renders a
dedicated `TaskbarView`:

```tsx
const sharedProps = {
  snapshot: radar.snapshot,
  status: radar.status,
  error,
  onRefresh: radar.refresh,
  onHide: hideWindow,
};

return expanded ? (
  <DetailView
    {...sharedProps}
    onCollapse={collapse}
    onOpenSource={() => openSourceSite(desktop.preferences.radarSource)}
  />
) : (
  <CompactView {...sharedProps} onExpand={expand} />
);
```

- `App` owns `expanded` and `windowError` because those are presentation/window concerns, not radar data.
- `CompactView` renders the leader summary and emits expand, refresh, and hide actions.
- `DetailView` renders the leader, four metrics, at most five ranking rows, attribution, and collapse/refresh/hide/source actions.
- `IconButton` is the shared primitive for icon-only commands.

Compact and detail are mutually exclusive in the main Tauri window. The
taskbar WebView must never mount either tree or a second detail window.

`TaskbarView` is presentational and fixed at `168 x 30`. Row one renders
model/effort/status; row two renders IQ/value/ties. Its primary click emits
`onShowDetails`; context-menu input prevents the WebView menu and emits
`onOpenContextMenu` for the shared native menu.

## Props Contract

All shared props live in `src/types/radar.ts`:

```ts
export type RadarAction = () => void | Promise<void>;
export type OpenSourceAction = () => void | Promise<void>;

export interface RadarViewProps {
  snapshot: RadarSnapshot | null;
  status: RadarStatus;
  error?: string | null;
  onRefresh: RadarAction;
  onHide: RadarAction;
}

export interface CompactViewProps extends RadarViewProps {
  onExpand: RadarAction;
}

export interface DetailViewProps extends RadarViewProps {
  onCollapse: RadarAction;
  onOpenSource: OpenSourceAction;
}
```

Use semantic action names (`onRefresh`, `onCollapse`) rather than passing setters or Tauri handles. Action props deliberately allow synchronous test doubles and asynchronous native implementations.

`IconButtonProps` extends the native button contract but removes `children`:

```ts
export interface IconButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  icon: LucideIcon;
  label: string;
  iconSize?: number;
}
```

It always renders `type="button"`, `aria-label`, `title`, and an `aria-hidden` Lucide icon. Keep these guarantees when adding icon controls.

## Rendering Rules

### Compact view

- Resolve leaders by matching `snapshot.leaderIds` against `snapshot.rankings`.
- If no identifier matches, fall back to only `rankings[0]`; never synthesize a model.
- Show one primary leader and a `+N` tie count.
- Use `getModelDisplayName` so reasoning effort remains in its separate field.
- The entire `.compact-summary` is a button that expands the current window.

### Detail view

- Render `rankings.slice(0, 5)` so the fixed ranking grid never grows.
- Determine tie ranks by the first index with the same numeric score. Equal scores receive the same displayed rank.
- Prefer source-provided human task duration, then derive from seconds, then show `--`.
- Render null/non-finite metrics as `--`; format USD only after a finite-number check.
- Always render the attribution footer. `DetailView` invokes a zero-argument
  semantic `onOpenSource`; `App` closes over the hydrated `radarSource` and
  passes it to the fixed enum-to-URL `openSourceSite(source)` mapping. Remote
  `attribution.url` and `sourceUrl` are display/data fields, never opener input.

## Window Actions And Errors

`changeWindowMode` updates React state only after `set_window_expanded` succeeds:

```ts
try {
  await setWindowExpanded(nextExpanded);
  setExpanded(nextExpanded);
  setWindowError(null);
} catch {
  // App.tsx sets localized windowError copy and leaves `expanded` unchanged.
}
```

This ordering keeps the mounted layout consistent with the native dimensions. A failed resize must not switch views. `Escape` collapses only while expanded, and the listener must be removed in the effect cleanup.

The displayed error precedence is `windowError ?? userFacingError(radar.error)`. Keep native window failures visible instead of immediately masking them with a data error.

## Accessibility Contract

- Each root surface has a descriptive `aria-label`, `data-state`, and `aria-busy` for `booting`/`refreshing`.
- Status changes use `role="status"`; detailed sync text and compact status text use polite live announcements where implemented.
- `TaskbarView` keeps its visible status inside the primary button with
  `aria-hidden="true"` and renders the polite `role="status"` live region as a
  visually hidden sibling. Interactive descendants must not contain the live
  region because button semantics can flatten descendant roles.
- Icon-only controls go through `IconButton` and therefore always have an accessible label and hover title.
- Decorative Lucide icons use `aria-hidden="true"`; numeric IQ values receive explicit labels.
- Empty rankings use a status role rather than an unlabeled decorative placeholder.
- Keyboard focus uses the shared `button:focus-visible` outline. Do not remove it.
- Only intended header/brand regions carry `data-tauri-drag-region`. Interactive controls remain real buttons.

## Styling Contract

Use semantic global classes from `src/App.css`; do not add inline size styles. The DOM structure is tied to stable grid tracks such as `.compact-summary`, `.metric-grid`, and `.ranking-row`. Text-bearing grid children require `min-width: 0` plus overflow handling so long model names cannot resize the window.

Use Lucide icons for controls. Do not introduce hand-written control SVGs or text-filled pill buttons when an established icon exists.

## Good / Base / Bad Cases

- Good: a tied snapshot shows the first matching leader, a `+N` count, and every tied row as a leader in details.
- Base: `snapshot === null` shows loading/unavailable copy, `--` metrics, and the fixed empty ranking surface.
- Bad input: a missing leader ID match falls back to the first normalized ranking without throwing.
- Bad native action: a rejected resize leaves the current component mounted and exposes `windowError`.

## Wrong vs Correct

### Wrong

```tsx
<button onClick={() => invoke("hide_window")}>Hide</button>
```

### Correct

```tsx
<IconButton icon={EyeOff} label="Hide window" onClick={onHide} />
```

The correct form preserves component isolation, icon consistency, tooltip text, and an accessible name.
