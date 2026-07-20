# Implementation plan

1. Refresh the light and dark tokens in `src/App.css`, adding explicit
   selection, inset, and numeric-font tokens.
2. Bundle the reference Codex SVG and distributed Sol/Terra/Luna PNGs locally.
   Resolve marks from exact model-family tokens with Codex as the fallback,
   including explicit GPT-5.5/GPT-5.4 coverage. Add decorative, fixed-size
   marks to compact, detail leader, and ranking markup. Add a taskbar instance
   only if it preserves the exact two-row geometry.
3. Refine shared shell, brand, status, icon-button, and focus treatments.
4. Refine compact summary hierarchy and score separation without changing its
   `360 x 112` grid.
5. Refine detail leader, metric band, rankings, and source footer without
   changing the `400 x 520` layout tracks.
6. Refine settings segmented controls, option rows, CSS switches, and feedback
   states without changing component behavior.
7. Refine taskbar typography, status cues, hover feedback, and fixed two-row
   alignment inside `168 x 30` while keeping the surface transparent and
   borderless.
8. Extend resolver and component tests to cover Sol/Terra/Luna, GPT-5.5,
   GPT-5.4, unknown fallback, decorative semantics, accessible model names,
   and unchanged taskbar actions.
9. Run `pnpm lint`, `pnpm typecheck`, `pnpm test`, and `pnpm build`.
10. Launch the Tauri dev app and visually inspect compact, detail, settings, and
   taskbar at native size and 150% Windows scaling. Check both color schemes
   when the environment permits and retain screenshots for user review.

## Review gates

- Stop and correct any geometry shift, clipping, text overlap, lost focus
  ring, or missing drag region before presenting the preview.
- Do not commit or archive the visual direction until the user has inspected
  the prototype.

## Verification Results

- `pnpm lint`, `pnpm typecheck`, `pnpm test` (82 tests), and `pnpm build`
  passed against the final frontend.
- Rust fmt/check, clippy with denied warnings, and 74 tests passed because the
  same final bundle includes the position command added during visual review.
- A fresh `pnpm tauri build` produced both Windows MSI and NSIS installers.
- Native Windows screenshots at 150% DPI covered compact, detail, settings, and
  taskbar projections. Sol/Terra/Luna and Codex fallback marks rendered locally;
  the user inspected the result and clarified that the settings bottom content
  was available through the intended internal scroll region.
