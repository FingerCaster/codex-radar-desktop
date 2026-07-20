# Desktop visual polish redesign

## Goal

Make Codex Radar feel like a precise, recognizable desktop instrument while
remaining quiet, compact, and efficient for repeated scanning.

## Background

- The current UI is functionally complete but relies on nearly uniform flat
  gray surfaces, thin borders, and a small set of similarly weighted labels.
- Antigravity reviewed the current React/Tauri views and recommended a
  restrained "instrument panel" direction: colder neutral surfaces, clearer
  signal colors, tabular numeric emphasis, subtle inset definition, and
  stronger leader/ranking hierarchy.
- The main window has three projections and fixed native geometry: compact is
  `360 x 112`; detail and settings are `400 x 520`. The Windows taskbar
  projection is a separate fixed `168 x 30` renderer.

## Requirements

- R1: Establish a precision signal-console visual language using neutral
  graphite/fog surfaces plus cobalt, emerald, amber, and red semantic colors.
- R2: Strengthen hierarchy without marketing composition, decorative large
  cards, gradients, glow-heavy effects, or ornamental imagery.
- R3: Compact view must make the current leader and IQ score immediately
  scannable while keeping all four header actions and the existing expand
  action.
- R4: Detail view must distinguish the leader, metrics, and top-five ranking
  through bands, rails, separators, and numeric typography. It must not turn
  page sections or each metric/ranking row into floating cards.
- R5: Settings must improve selected states, row feedback, and checkbox
  affordance. Existing native preference behavior, labels, pending state,
  disabled state, error state, and keyboard access must remain intact.
- R6: Taskbar view must preserve its two-row projection and right-click menu
  while improving alignment and signal visibility inside `168 x 30`.
- R7: Provide coherent light and dark palettes with legible ready, booting,
  refreshing, stale, unavailable, saving, and error states.
- R8: Preserve all window geometry, native drag-region behavior, tooltips,
  focus visibility, reduced-motion handling, high-DPI behavior, and existing
  compact/detail/settings/taskbar interactions.
- R9: Implement the first reviewable prototype with the smallest practical
  frontend surface. Prefer a CSS-only change if visual QA confirms the current
  DOM supports the design.
- R10: Use model-family artwork from the reference sites as local repository
  assets. Map `gpt-5.6-sol`, `gpt-5.6-terra`, and `gpt-5.6-luna` to the
  distributed site's sun, earth, and moon artwork respectively. Use the Codex
  mark for GPT-5.5, GPT-5.4, and every unrecognized/default model family.
  Show the resolved mark beside model identity in compact, detail leader, and
  ranking views; include a reduced taskbar treatment only if the fixed two-row
  geometry stays intact. Images are decorative and must not duplicate
  accessible model text.

## Acceptance Criteria

- [x] Compact, detail, settings, and taskbar render at their exact fixed sizes
      with no clipping, overlap, or text overflow at Windows 150% scaling.
- [x] The leader/model, IQ score, status, and primary actions have an obvious
      visual order in both light and dark themes.
- [x] Settings segmented controls, switches, hover, focus, pending, disabled,
      and error states are visibly distinct and remain keyboard accessible.
- [x] Detail metrics and rankings remain dense continuous information bands,
      not nested or floating card grids.
- [x] Taskbar left click and right click retain their existing behaviors and
      the fixed two-row layout does not shift.
- [x] Existing frontend lint, typecheck, tests, and production build pass.
- [x] The locally bundled Sol/Terra/Luna and Codex marks resolve from stable
      model identifiers, render crisply in compact, detail leader, and ranking
      contexts, and cause no remote requests. GPT-5.5/GPT-5.4 explicitly keep
      the Codex mark. Taskbar use is retained only when native visual QA proves
      there is no truncation or layout shift.
- [x] Native screenshots of the reviewable prototype cover compact, detail,
      settings, and taskbar; the user can inspect the result before a final
      visual direction is committed.

## Out of Scope

- Changing radar data, IPC contracts, persistence, autostart, menus, or native
  window lifecycle.
- Changing window dimensions or introducing responsive viewport sizing.
- Downloading fonts, hotlinking remote assets, or adding imagery other than
  the user-requested local Codex and Sol/Terra/Luna model marks.
- Marketing layouts, large decorative cards, gradients, bokeh/orbs, or
  animation beyond existing status feedback.
