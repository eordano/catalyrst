---
id: bevy-overlay-community-create
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    In the in-client (bevy-overlay) communities panel, gating community creation
    behind an explicit NAME check and then walking the creator through legible
    steps (gate -> profile -> details -> review) increases the share of opened
    create panels that reach a completed (simulated) create, even with the final
    POST simulated.
  because: >-
    The Unity client surfaces creation as a single dense CreateOrEditCommunityPanel
    modal over the directory; making the NAME requirement explicit up front (so
    creators without a NAME bail early instead of at submit) and splitting the
    remaining fields into one decision per screen reduces mid-flow drop-off, so
    more name-holding creators push through to a finished create.
metric:
  primary: cl_community_create_completion_rate
  numerator: cl_community_created
  denominator: cl_community_create_opened
  guardrails:
    - cl_community_create_opened
    - cl_community_create_submit_failed
experiment:
  key: cl_community_create_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.4
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if cl_community_create_completion_rate improves by at least the MDE with
    no guardrail regression (panel-open volume holds and the simulated submit
    failure path stays rare/graceful); otherwise hold.
---

# Create a community -- in-client overlay (create SIMULATED)

The bevy-overlay community-create panel (`/bevy-overlay/community-create`) opens
the `CreateOrEditCommunityPanel` modal over the dimmed Communities directory and
walks the creator through the steps that map 1:1 to the upstream
`createCommunityHandler` form fields:

- **create** -- open the create panel over the dimmed directory.
- **gate** -- the `GetNamePanel` NAME-gate ("Get a NAME to Unlock Community
  Creation"); shown when the session lacks a claimed DCL NAME
  (`require_owned_name`), **skipped** when the session already owns one
  (`?owned=1`).
- **profile** -- PROFILE PICTURE (512x512 thumbnail), optional.
- **details** -- COMMUNITY NAME + MEMBERSHIP (Public/Private = `privacy`) +
  DISCOVERABILITY (listed/unlisted = `visibility`).
- **review** -- review + content-policy **acknowledgement** (required).
- **submit** -- SIMULATED `create_community` (`POST /v1/communities`).
- **done** -- created confirmation.

This story tracks whether the gated, stepped overlay increases the share of
opened create panels that reach a completed (simulated) create.

- **Primary metric:** `cl_community_create_completion_rate` =
  `cl_community_created` / `cl_community_create_opened`.
- **Guardrails:** panel-open volume (`cl_community_create_opened`) and the
  simulated submit-failure path (`cl_community_create_submit_failed`) must stay
  healthy.
- **Events:** `cl_community_create_opened` (panel opened),
  `cl_community_gate_viewed` (entering the NAME gate),
  `cl_community_gate_passed` (`{had_name}`, leaving the gate),
  `cl_community_step_completed` (`{from,to}` per forward NEXT),
  `cl_community_review_reached` (entering review),
  `cl_community_submit_attempted` (review -> submitting),
  `cl_community_create_submit_failed` (`{error}`, simulated error -> back to
  review), `cl_community_created` (stub, reaching done).

Data reality: `catalyrst-communities` exists in the workspace
(`writes::create_community`) but the route is **not proxied on the public edge**
(`GET /social/v1/communities` and `/v1/communities` both 404), and
the real create needs a DCL auth-chain signature plus a held NAME, so the final
create is **SIMULATED** via an XState actor -- flow, states, the NAME gate, field
validation, the content-policy acknowledgement and metrics are real; only the
on-chain/HTTP commit is stubbed. Noted as deferred.
