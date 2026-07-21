# PRD: Harden desktop recovery when taskbar companion fails

## Problem

Users can end up with no visible UI:

1. `showMainWindow: false` and only the taskbar companion enabled
2. Taskbar companion missing, invisible, or detached from Explorer
3. Main window still hidden and/or parked off-screen
4. Tray recovery then feels “broken” (main never appears in a findable place)

## Goal

Make Windows desktop recovery self-healing:

- Prefer re-creating a healthy taskbar companion when the preference is still on
- On unrecoverable taskbar failure: disable taskbar projection, force main visible, clamp main on-screen
- Never hide the main window until taskbar show/placement has succeeded
- Tray / show-main paths that surface the main window must also re-clamp to a work area

## Acceptance criteria

1. `apply_visibility` for taskbar-only never leaves both surfaces unavailable after a failed taskbar create/place/show.
2. Taskbar monitor tries one recreate when the companion window is missing but still preferred.
3. Unhealthy companion (detached/invisible after place) demotes with main shown on-screen.
4. Recovery show path clamps the main window into an available work area (multi-monitor disconnect safe).
5. Unit tests cover preference demotion, visibility ordering helpers / pure recovery preference rules, and reclamp selection.
6. Spec contract documents the new recovery rules.

## Out of scope

- Linux
- Classic / secondary taskbar embedding
- Redesign of taskbar geometry algorithm
