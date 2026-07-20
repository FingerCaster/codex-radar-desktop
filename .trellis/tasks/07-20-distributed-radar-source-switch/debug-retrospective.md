# Bug Analysis: A-B-A source reactivation race

## 1. Root Cause Category

- **Category**: E - Implicit Assumption, with a D - Test Coverage Gap contributor.
- **Specific Cause**: The first source-isolation design treated `RadarSource` as
  both the stable source identity and the identity of the current activation.
  After `main -> distributed -> main`, an old main request saw `main` active
  again and could regain authority to join, commit, publish, or affect the new
  activation's notification baseline. Source-local caches prevented cross-source
  contamination, but enum equality could not distinguish old `main A1` from
  reactivated `main A2`. The first generation fix still tracked only the active
  token: after `A0 -> B1 -> A2 -> B3`, A0 could write A's private cache because
  B was active. It also did not cover a command result already queued after
  Rust returned, or listener-first hydration where a delayed initial read could
  overwrite a newer event.

### Evidence and confidence

Initial hypotheses were: cross-source cache leakage (40%), incomplete active
source checks (35%), and single-flight ownership/cleanup (25%). Tests showed
validators and caches were already isolated by source, reducing cache leakage.
The discriminating `A1 -> B1 -> A2` schedule reproduced the failure even though
the active enum matched A at completion. That raised activation identity to the
root cause with high confidence. A delayed old caller also demonstrated that a
single replaceable flight slot could disrupt current-generation joining. The
expanded `A0 -> B1 -> A2 -> B3` schedule disproved the assumption that checking
only the current token protected inactive private state. Deferred WebView
promises and event-before-initial-read tests then isolated two post-Rust and
frontend hydration windows.

## 2. Why Fixes Failed

1. **Per-source runtimes**: They fixed cross-source cache and validator leakage,
   but did not distinguish two activations of the same source.
2. **Rechecking only the active enum**: This rejected `A -> B` late results, but
   accepted the same result after `A -> B -> A` because the stable identity
   matched again.
3. **One flight cell per source**: Replacing that cell for A2 allowed delayed A1
   cleanup or joining behavior to interact with A2. Flight ownership needed the
   activation generation as part of its key.
4. **Only an active-token generation check**: It protected publication but did
   not remember that A0 had been permanently superseded once A2 existed.
5. **Only a Rust return-time check**: It could not retract a success already
   queued in the WebView before the native menu completed A -> B -> A.
6. **Concurrent listener registration and initial reads**: A newer preference
   or expanded-state event could lose to an older initial command result.

## 3. Prevention Mechanisms

| Priority | Mechanism | Specific Action | Status |
| --- | --- | --- | --- |
| P0 | Architecture | Represent active selection as `RefreshToken { source, generation }` and compare the full token before any visible side effect | DONE |
| P0 | Architecture | Key per-source single-flight cells by generation and remove only the exact `Arc` owned by the finishing caller | DONE |
| P0 | Architecture | Record each source's latest generation and commit snapshot/validator together under one `SourceState` lock | DONE |
| P0 | Cross-layer guard | Recheck every joined Rust caller, then use a renderer-session activation epoch to reject post-return Promise results | DONE |
| P0 | Test coverage | Exercise delayed success/failure, join isolation, cleanup, and notification baseline across `A1 -> B1 -> A2` | DONE |
| P0 | Initialization ordering | Register event listeners before initial reads; once an event arrives, ignore delayed initial success/failure | DONE |
| P1 | Documentation | Record stable identity versus activation identity in the radar contract and cross-layer thinking guide | DONE |
| P1 | Transaction design | Persist menu/preferences while holding the active-selection write guard; advance generation only after a successful commit | DONE |

## 4. Systematic Expansion

- **Similar Issues**: Any source, account, workspace, profile, tenant, or endpoint
  switch that can return to a prior enum/string value can have the same race.
- **Design Improvement**: Async authority should be represented by a monotonic
  activation token. Cache identity and publication authority are separate
  concepts and should not share one key by accident.
- **Process Improvement**: Every switchable async feature review must include an
  `A1 -> B1 -> A2` and `A0 -> B1 -> A2 -> B3` schedule, not only the simpler
  `A -> B` stale-response case. Listener-plus-read initialization must include
  event-first success and failure tests.
- **Knowledge Gap**: Single-flight deduplication is safe only when its key covers
  every state dimension that changes result authority.

## 5. Knowledge Capture

- [x] Update the backend radar data contract with generation-keyed activation,
  publication, cleanup, and notification-baseline rules.
- [x] Update frontend hook/state/type specs for the `superseded` no-op contract.
- [x] Add the stable-identity versus activation-identity checklist to the
  cross-layer thinking guide.
- [x] Add Rust and TypeScript regression tests for the A-B-A schedule.
- [x] Add Rust tests for permanent per-source supersession, atomic cache state,
  and late joined callers.
- [x] Add renderer tests for batched activation epochs, deferred command
  success/failure, preference hydration, and expanded-state hydration.
- [ ] Validate native Windows and macOS behavior manually; this remains with the
  user by request and does not change the concurrency proof.
