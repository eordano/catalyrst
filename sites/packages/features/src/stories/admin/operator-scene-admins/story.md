---
id: operator-scene-admins
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided manage-admins flow (pick the operated place -> review explicit and
    implicit grants -> add/revoke -> confirm) increases the share of started
    grant actions that reach the confirm step, even with the commit simulated.
  because: >-
    Showing the operator exactly which place they are editing, which grants are
    removable (explicit) versus inherited (extra / land-lease, non-removable),
    and an explicit confirm of the address + place reduces uncertainty, so more
    operators who start a grant/revoke push through instead of bailing at an
    opaque single-shot edit.
metric:
  primary: operator_admin_grant_confirm_rate
  numerator: operator_admin_grant_committed
  denominator: operator_admin_grant_started
  guardrails:
    - operator_admin_action_failed
experiment:
  key: operator_scene_admins_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Not shippable as an experiment while the endpoints are unreachable from the
    edge. The readout below is retained as the intended design.
---

# Manage scene admins (/scene-admin add/remove grants)

`/operator/scene-admins` currently renders **no grant list and no wizard**. Two
different things live on this page and they must not be blurred.

## The place list is public

`GET /places/api/places?owner=` is unauthenticated:
`catalyrst-places/src/handlers/places.rs:66-73` (`get_place_list`) calls
`crate::auth::auth_address_optional` and gates nothing. Anyone gets the same
answer for any address.

`?owner=` therefore survives as what it is -- a filter over public data -- but
relabelled and demoted. The page says "viewing places registered to <address>",
never "your places", and surfaces the built-in `DEMO_OWNER` fallback as a demo
address rather than as the viewer. The address is not an identity claim and
confers nothing.

## The grants are BLOCK

The server-side checks are real and correct:

- list -- `catalyrst-comms/src/handlers/scene_admin.rs:56-62`
- grant -- `catalyrst-comms/src/handlers/scene_admin.rs:123-131`
- revoke -- `catalyrst-comms/src/handlers/scene_admin.rs:145-157`
- all three -> `ports/scene_perms.rs:16-114`, which denies on pool failure
  (`:27-34`)

They are unreachable from this node. There is no nginx `location` for
`/scene-admin`, and the correct public path `/comms/scene-admin` is used
nowhere. Adding that edge route is a deployment config change and is
deliberately not part of a UI change.

`scene-admins.server.ts` used to hardcode `grants: []`, which displayed as "this
place has no scene admins" above Add and Revoke buttons that could never work.
It now returns the unavailable reason, and the page renders the server-side
check it is subject to, why it cannot be reached, and disabled controls carrying
that reason. `operator_scene_admins_viewed` has been dropped from the telemetry
catalog; the page emits `operator_control_unavailable { control, reason }`.

`ManageAdminsWizard` and its machine are left in place, unrendered, for when the
edge route exists.
