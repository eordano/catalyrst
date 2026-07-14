---
id: operator-scene-bans
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A per-place ban/unban list (pick a place -> see who's banned -> add or remove
    a banned wallet -> confirm) raises the share of scene operators who start a
    ban action and push it through to a committed ban/unban, versus an opaque
    one-shot moderation control.
  because: >-
    Showing the current banned list for the chosen place, plus an explicit
    confirm step that echoes the exact address and place, reduces the fear of
    banning the wrong wallet -- so operators who start a moderation action finish
    it instead of bailing before commit.
metric:
  primary: operator_scene_ban_commit_rate
  numerator: operator_scene_ban_committed
  denominator: operator_scene_ban_started
  guardrails:
    - operator_scene_bans_viewed
    - operator_scene_ban_failed
experiment:
  key: operator_scene_bans
  unit: session
  variants:
    - id: list
      weight: 1
      flags:
        confirmStep: true
  baseline: 0.45
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Not shippable as an experiment while the endpoints are unreachable from the
    edge. The readout below is retained as the intended design.
---

# Scene Operator -- scene ban / unban list (/scene-bans per-place moderation)

`/operator/scene-bans` currently renders **no ban list and no ban form**.

## BLOCK -- real checks, unreachable endpoints

- list -- `catalyrst-comms/src/handlers/scene_bans.rs:88-89` `verify_signed_fetch`
- ban -- `catalyrst-comms/src/handlers/scene_bans.rs:170-178`
- unban -- `catalyrst-comms/src/handlers/scene_bans.rs:193-205`
- both writes -> `ports/scene_perms.rs:16-114`, denying on pool failure
  (`:27-34`)

The checks are correct and fail closed. The problem is reachability: exactly as
for scene admins, nginx has no `location` for `/scene-admin` and
`/comms/scene-admin` is used nowhere. That is a deployment change, not a UI
change.

## The fixture no longer stands in for a live answer

What was here before: the loader called the live endpoint, a bare `catch {}`
swallowed the 401, and `src/fixtures/operator-scene-bans.json` supplied ban rows
tagged `source: "fixture"`. A moderator saw plausible banned wallets for a real
scene, produced by a JSON file, above a working-looking ban form.

`loadSceneBansPage` now makes no request and returns the unavailable reason. The
fixture is **not** deleted -- it is still the place list for this layout -- but
`loadOperatorPlaces()` returns `synthetic: true` and the page renders a
persistent "sample place list" banner saying those places come from a JSON file,
not from the network.

Ban and unban render as disabled controls carrying the reason. The page emits
`operator_control_unavailable { control, reason }`; `SceneBanWizard` and its
machine are left in place, unrendered, for when the edge route exists.
