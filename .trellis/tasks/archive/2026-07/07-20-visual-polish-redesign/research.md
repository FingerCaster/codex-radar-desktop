# Visual research

## Repository evidence

- `src/App.css` owns the complete visual system for all four projections.
- The current DOM already exposes the necessary semantic hooks: leader band,
  metric band, ranking rows, segmented controls, checkbox inputs, taskbar
  rows, status attributes, and fixed view classes.
- Existing tests protect drag markers, accessible actions, settings behavior,
  and taskbar left/right click semantics. No visual component contract needs
  to change for the first prototype.

## Antigravity findings

Antigravity inspected the current components and full light/dark CSS. Its
recommended direction was a restrained technical instrument panel:

- cold gray/graphite surfaces with cobalt action color and emerald score color
- tabular/monospaced score typography
- subtle inset definition instead of flat one-pixel boxes
- stronger leader rail and row emphasis
- modern switch styling for the existing checkbox inputs
- better grid alignment in the fixed taskbar renderer

The recommendation to make each metric and ranking row a small rounded card is
intentionally not adopted. Continuous bands and separators better preserve
the operational density and avoid card nesting.

## Implementation conclusion

The first prototype began as a CSS-only change in `src/App.css`. The later
model-icon requirement needs a narrowly scoped component change while keeping
accessibility, actions, native drag, IPC, and data contracts unchanged.

## Reference model marks

- Kimi WebBridge inspected `https://codexradar.com/` in the user's browser.
- The page's model ranking rows do not contain separate provider images. Its
  only Codex/model identity image is `assets/codex-logo.svg` (`128 x 128`
  viewBox; displayed as the site's Codex mark).
- The SVG contains a purple-blue terminal-cloud mark on a light rounded tile.
- After the first prototype, the user supplied the distributed-radar reference
  and clarified the family mapping. Kimi WebBridge inspected
  `https://deng.codexradar.com` and found the exact transparent `512 x 512`
  assets used by its IQ cards:
  - `assets/orbs/sol-transparent.png?v=2`
  - `assets/orbs/terra-transparent.png?v=2`
  - `assets/orbs/luna-transparent.png?v=2`
- The corresponding cards identify their models as `gpt-5.6-sol`,
  `gpt-5.6-terra`, and `gpt-5.6-luna`. Mapping should use the normalized model
  identifier when available, with the visible model name only as the taskbar
  projection fallback. GPT-5.5, GPT-5.4, and unknown families use the Codex
  mark.
- All four sources should be copied into `src/assets/` and imported through
  Vite so the desktop app never depends on either website at runtime.
- Instances beside model names should use empty alt text and `aria-hidden`;
  the adjacent model name remains the single accessible identity.
