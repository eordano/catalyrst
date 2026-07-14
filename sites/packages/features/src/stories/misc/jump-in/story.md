---
id: jump-in
status: running
owner: owner@example.com
hypothesis:
  statement: >-
    Adding an explicit "confirm" step before launching a place will increase the
    rate of completed jump-ins by reducing accidental launches and giving the
    visitor a moment of clear intent.
  because: >-
    Visitors who click "JUMP IN" by accident bounce immediately; a lightweight
    confirm dialog (TeleportPrompt) re-states the destination so only intentful
    visitors proceed, raising the share of launches that actually complete.
metric:
  primary: jump_in_completed_rate
  numerator: jump_in_completed
  denominator: jump_in_started
  guardrails:
    - jump_in_started
    - jump_in_failed
experiment:
  key: jump_in_confirm
  unit: session
  variants:
    - id: control
      weight: 50
      flags:
        confirmStep: false
    - id: treatment
      weight: 50
      flags:
        confirmStep: true
  baseline: 0.42
  mde: 0.05
  min_sample: 7400
decision:
  rule: >-
    Ship treatment if jump_in_completed_rate is higher than control with 95%
    confidence and no guardrail (jump_in_started_rate, jump_in_failed_rate,
    time_to_jump_in_ms) regresses beyond its tolerance; otherwise keep control.
---

# Story B -- View a place & jump in

Visitor lands on a place detail page and launches into the world. We test
whether a confirmation step (the `TeleportPrompt`) before launch improves the
share of started jump-ins that complete.

- **control** (`confirmStep: false`): clicking JUMP IN launches immediately --
  `idle -> launching`.
- **treatment** (`confirmStep: true`): clicking JUMP IN opens the confirm
  dialog first -- `idle -> confirming -> launching`.

The launch step resolves the realm / launch URL from `{CATALYST_URL}/about` and
then hands off to the Decentraland client.
