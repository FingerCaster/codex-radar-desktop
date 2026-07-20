# Design: precision signal console

## Direction

Use a compact "precision signal console" language. The interface should feel
calibrated and technical, not futuristic decoration: crisp geometry, quiet
neutral surfaces, deliberate semantic color, and strongly aligned numbers.

## Token strategy

- Light foundation: fog-gray canvas, white raised surfaces, cool gray borders,
  near-black text.
- Dark foundation: neutral graphite canvas and raised charcoal surfaces rather
  than a blue-only theme.
- Cobalt communicates actions, loading, focus, and selected controls.
- Emerald communicates valid scores and synchronized/leader state.
- Amber communicates stale data and the trophy/rank-one signal.
- Red is reserved for unavailable/error state.
- Add local system numeric font tokens; do not load external fonts.

## View treatment

### Compact

- Keep the exact two-row layout and header actions.
- Place the local Codex model mark at the leading edge of the summary without
  stealing width from the score block or action row.
- Define the summary with a subtle inset edge and a persistent leader signal
  rail rather than another decorative container.
- Separate the IQ block with a quiet divider and use tabular numeric emphasis.
- Give the radar mark and icon controls clearer hover/active/focus surfaces.

### Detail

- Treat the leader area as the primary signal band with an emerald rail and an
  amber trophy cue.
- Add the local mark to the leader identity and each ranking model cell using
  fixed icon tracks so long names still ellipsize predictably.
- Keep metrics as one continuous recessed band with calibrated separators.
- Use a soft leader-row wash plus a stronger rank rail; keep all ranking rows
  flat, aligned, and scan-friendly.
- Preserve the source footer as an unframed utility band.

### Settings

- Keep full-width unframed sections and improve heading rhythm.
- Make segmented selected states high-contrast and keyboard-visible.
- Restyle the existing checkboxes as switches using CSS only, including
  checked, disabled, hover, and focus-visible states.
- Use row hover feedback without wrapping options in cards.

### Taskbar

- Keep the fixed two-row DOM and current actions.
- Keep the surface transparent and borderless. Use only non-layout-consuming
  status cues, typography, and a restrained translucent hover response so the
  renderer remains visually integrated with the Windows taskbar.
- Tighten numeric and status alignment without changing the renderer bounds.
- A miniature mark may occupy a fixed non-growing track before the model name
  only when the full `168 x 30` renderer remains legible at native DPI.

## Asset treatment

- Import the Codex SVG plus the distributed Sol/Terra/Luna transparent PNGs
  locally through the frontend bundler; do not hotlink either reference site.
- Resolve the asset from the model identifier: exact normalized family tokens
  `sol`, `terra`, and `luna` select their celestial artwork; all other models,
  including GPT-5.5 and GPT-5.4, select the Codex mark. The taskbar projection
  may use the normalized display name because it intentionally carries only a
  user-facing model label.
- Render it as decorative (`alt=""`, `aria-hidden="true"`) because adjacent
  text already names the model.
- Keep icon boxes square and fixed-size so loading or status changes cannot
  shift the surrounding layout.
- The user-requested source SVG retains its internal gradient fills, and the
  source PNGs retain their baked transparent glow. No new CSS gradients,
  glows, or decorative imagery are introduced elsewhere.

## Compatibility

- Component markup gains only decorative images and fixed layout wrappers; no
  props, interactive semantics, data contracts, or native APIs change.
- Exact native geometry remains authoritative.
- `prefers-color-scheme` and `prefers-reduced-motion` remain supported.
- All UI effects use CSS borders, inset shadows, solid colors, and color
  mixing; only the bundled reference assets contain gradients or baked glow.

## Rollback

The prototype remains frontend-only: remove the local marks, their decorative
image instances and resolver, and the corresponding CSS. No state or migration
rollback is needed.
