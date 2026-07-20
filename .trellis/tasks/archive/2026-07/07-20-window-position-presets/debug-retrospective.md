# Bug Analysis: Wry getter and geometry-gate deadlock

## 1. Root Cause Category

- **Category**: E - Implicit assumption, with D - Test coverage gap.
- **Specific cause**: The initial design treated public Tauri window getters as
  ordinary thread-safe reads. In Wry, an off-main-thread getter posts work to
  the main event loop and blocks on `recv()`. Holding `main_position_gate`
  across that call allowed a main-thread preset/close/exit callback to wait for
  the gate while the writer waited for the callback's event loop.

## 2. Why The Initial Design Failed

1. Ordinary Rust lock-order review found no `gate -> state` inversion, but did
   not include the hidden `getter -> main event loop` dependency.
2. Unit tests exercised pure geometry and writer flags without a real Wry event
   loop, so compilation and 45 passing tests could not expose the ABBA cycle.
3. The same review found a writer-error lost wakeup and mixed-DPI restore gap;
   both required reasoning across native events, state revisions, and monitor
   scaling rather than isolated helper correctness.

## 3. Prevention Mechanisms

| Priority | Mechanism | Specific Action | Status |
|---|---|---|---|
| P0 | Architecture | Capture background geometry before the gate, then gate + revision recheck + write | DONE |
| P0 | Architecture | Marshal complete resize transactions to the main event loop without holding the gate while waiting | DONE |
| P1 | State machine | Atomically retain/restart the writer when a newer revision arrives during failure cleanup | DONE |
| P1 | Test coverage | Add lost-wakeup and mixed-DPI restore regressions | DONE |
| P1 | Code review | Add Wry thread-affinity checks to backend desktop specs and quality guidelines | DONE |

## 4. Systematic Expansion

- **Similar issues**: Any background `scale_factor`, `outer_position`,
  `outer_size`, monitor, or setter call made while holding a mutex that a main
  tray/menu/window/run callback may acquire.
- **Design improvement**: Treat event-loop affinity as part of the lock graph;
  a public thread-safe handle does not imply a non-blocking native operation.
- **Process improvement**: Native concurrency reviews must inspect the pinned
  runtime implementation when lock safety depends on whether an API marshals to
  the main thread.

## 5. Knowledge Capture

- [x] Updated backend desktop contract with Wry getter/thread rules, position
  persistence, presets, mixed-DPI restore, and writer failure behavior.
- [x] Updated backend quality guidelines with the concurrency review trigger.
- [x] Updated frontend companion contract and task technical design.
- [x] Added focused Rust regressions.
- [x] Template sync is not applicable: this application repository has no
  `src/templates/markdown/spec/` or Trellis template source tree.
- [x] Commit is not applicable in this workspace root because it is
  intentionally not a Git repository.
