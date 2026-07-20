# Frontend Quality Guidelines

> Quality gates cover strict TypeScript, deterministic reducer behavior, accessible fixed-size UI, and the renderer/Tauri boundary.

## Required Commands

The scripts in `package.json` are the canonical frontend checks:

```powershell
pnpm lint       # eslint . --max-warnings 0
pnpm typecheck  # tsc --noEmit
pnpm test       # vitest run
pnpm build      # pnpm typecheck && vite build
```

`eslint.config.js` applies JavaScript, TypeScript, React Hooks, and React Refresh rules to `src/**/*.{ts,tsx}`. It ignores `dist/`, `node_modules/`, `src-tauri/`, and `.trellis/`. Do not treat an ignored file as evidence that renderer code is checked.

Vitest is configured in `vite.config.ts` with `jsdom`, `globals: true`, CSS processing enabled, mock clearing, mock restoration, and the include pattern `src/**/*.{test,spec}.{ts,tsx}`.

## Test Organization And Current Assertions

Tests are colocated with the unit under test:

| File | Contract currently covered |
|---|---|
| `src/App.test.tsx` | main expanded-state listener-before-read ordering and event-wins success/failure hydration |
| `src/hooks/useDesktopPreferences.test.ts` | listener-before-read hydration, event-wins ordering, late unlisten cleanup, and source activation epoch increments |
| `src/hooks/useRadar.test.ts` | source selection/cache transitions; same-source retention/staleness; delayed cross-source success/failure rejection |
| `src/hooks/useRadar.runtime.test.tsx` | disabled hydration barrier, passive renderer, active recovery, synchronous old-source masking, and deferred A-B-A command rejection |
| `src/lib/radar.test.ts` | source-specific v2 round-trip, fixed-main v1 migration, malformed/cross-source rejection, failure source, fixed opener mapping |
| `src/lib/model.test.ts` | reasoning suffix is removed from display label only when it is an actual suffix |

Use `src/test/fixtures.ts:sampleSnapshot` as the shared valid normalized fixture. Clone it with object spread for one-field invalid cases rather than duplicating a large fixture.

When changing state transitions, assert status, snapshot identity, and error kind. When changing a guard, test both the accepted shape and the smallest malformed value that should be rejected. Component tests should assert accessible names and action calls, not implementation CSS selectors alone.

## CSS And Layout Invariants

`src/App.css` uses global CSS variables and logical CSS pixels. Preserve these fixed geometry contracts:

| Surface/control | Stable dimensions or tracks |
|---|---|
| `.compact-view` | `360px x 112px`, rows `26px 64px`, `8px` padding |
| `.compact-summary` | `64px` height, columns `minmax(0, 1fr) 86px 18px` |
| `.icon-button` | `26px x 26px`, `flex: 0 0 26px` |
| `.detail-view` | full available window, rows `42px minmax(0, 1fr)` |
| `.detail-content` | rows `96px 84px minmax(0, 1fr) 60px` |
| `.taskbar-view` | fixed `168px x 30px` |
| `.taskbar-surface` | rows `12px 14px` plus `2px` vertical padding; transparent and borderless |
| `.metric-grid` | four equal columns |
| `.ranking-list` | exactly five equal rows |
| `.ranking-row` | columns `27px minmax(0, 1fr) 43px 54px` |

All text-bearing grid children use `min-width: 0`, overflow clipping, and ellipsis where appropriate. Numeric values use `font-variant-numeric: tabular-nums`. `letter-spacing` is explicitly zero globally. Do not introduce viewport-scaled font sizes, content-dependent window dimensions, or flex tracks that can grow the native window.

## High-DPI And Accessibility Behavior

The native window asks Tauri for logical sizes (`360 x 112` compact and `400 x 520` expanded). CSS dimensions are therefore logical WebView pixels, not hard-coded physical pixels. Rust caps the expanded logical size to the monitor work area after dividing by the OS scale factor; the detail `minmax(0, 1fr)` track absorbs the reduced height.

At 125%, 150%, and 200% display scaling, verify that:

- the compact surface remains one row of controls and the score does not overlap the model;
- the detail ranking list remains inside the viewport even when the native expanded height is capped;
- long model names, attribution text, and metric values ellipsize rather than resizing controls;
- `button:focus-visible` remains visible;
- dark-mode colors and reduced-motion behavior still apply.
- the taskbar child remains two complete rows, with no native size clamp below
  its CSS viewport and no opaque card background.

The stylesheet follows `prefers-color-scheme: dark` and `prefers-reduced-motion: reduce`; the latter slows, rather than removes, the refresh spinner.

## Required UI States

Every view must remain renderable for all `RadarStatus` values and for `snapshot === null`:

- `booting`: connection copy, busy state, no fabricated model;
- `ready`: current snapshot and normal status;
- `refreshing`: current snapshot retained if present, refresh control disabled/spinning;
- `stale`: last-known-good data plus warning copy;
- `unavailable`: explicit offline copy when no snapshot exists.

Every state also has a source invariant: a visible snapshot and failure must
match the hydrated desktop source. During the render before a source-selection
effect commits, the old snapshot is synchronously masked as booting.

Manual refresh must remain available in stale and unavailable states. Detail view must still render attribution and a source action when data is present.

## Forbidden Patterns

- Raw upstream payload parsing in `src/components/` or `src/hooks/`.
- Clearing `snapshot` on refresh start or failure.
- A second polling interval in React.
- Unbounded ranking rows or content-driven window sizing.
- Inline SVG control icons when an equivalent Lucide icon exists.
- Icon-only buttons without `aria-label` and `title`.
- Removing `overflow: hidden`, `min-width: 0`, or focus outlines to fit one unusually long label.
- Calling arbitrary URLs from the source button; the opener remains fixed to the allowlisted source site.
- Reading the default main cache before `DesktopPreferences.radarSource` is hydrated.
- Comparing timestamps or retaining last-known-good data across different sources.
- Silencing lint/type/test failures by weakening `tsconfig`, adding `eslint-disable`, or changing Vitest include patterns.

## Review Checklist

- Does the change stay within the directory ownership rules?
- Are all new props and state transitions represented by shared types or the reducer union?
- Does every asynchronous listener have late-registration cleanup?
- Does malformed/older data preserve the last-known-good snapshot?
- Do source, cache key, action, payload, and failure discriminators agree before state changes?
- Does every actual source transition increment the local activation epoch, and
  do awaited command continuations verify the captured activation?
- Can a source change paint the prior source for one frame or let the passive renderer refresh?
- Are source attribution and fixed opener behavior intact?
- Do compact and detail dimensions remain stable at high DPI?
- Are accessible labels, busy/status announcements, focus, and keyboard collapse preserved?
- Are focused unit tests added and `pnpm lint`, `pnpm typecheck`, `pnpm test`, and `pnpm build` expected to pass?

## Wrong vs Correct

### Wrong

```css
.ranking-list {
  display: flex;
}

.ranking-row {
  padding: 1vw;
  font-size: 2vw;
}
```

### Correct

```css
.ranking-list {
  display: grid;
  grid-template-rows: repeat(5, minmax(0, 1fr));
  min-height: 0;
}

.ranking-row {
  display: grid;
  grid-template-columns: 27px minmax(0, 1fr) 43px 54px;
  min-width: 0;
}
```

The correct layout remains bounded and readable under OS scaling and long labels.
