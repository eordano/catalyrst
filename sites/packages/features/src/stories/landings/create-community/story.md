---
id: landings-create-community
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Splitting community creation into explicit, legible steps (sign-in gate ->
    basics -> thumbnail -> privacy -> places -> review) increases the share of
    started community creations that reach the review/submit step, even with the
    final create simulated.
  because: >-
    A single dense modal asks for name, picture, membership, discoverability and
    places all at once behind a NAME gate; breaking it into one decision per
    screen reduces drop-off, so more creators who pass the gate push through to
    review instead of bailing at the wall of fields.
metric:
  primary: lp_community_review_rate
  numerator: lp_community_review_reached
  denominator: lp_community_started
  guardrails:
    - lp_community_started
    - lp_community_submit_failed
experiment:
  key: lp_community_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if lp_community_review_rate improves by at least the MDE with no
    guardrail regression (creation-start volume holds and the simulated submit
    failure path stays rare/graceful); otherwise hold.
---

# Create a community (create SIMULATED)

The create-community wizard (`/landings/create-community`) breaks community
creation into explicit steps that map 1:1 to the upstream
`createCommunityHandler` form fields: a **sign-in gate** (you need a claimed DCL
NAME -- `require_owned_name`), **basics** (name + description), **thumbnail**
(512x512 image), **privacy** (membership public/private = `privacy`;
discoverability listed/unlisted = `visibility`), **places** (attach owned places
= `placeIds`), then **review** before a **submitting** -> **created** finish.

This story tracks whether the stepped wizard increases the share of started
creations that reach the review step.

- **Primary metric:** `lp_community_review_rate` = `lp_community_review_reached`
  / `lp_community_started`.
- **Guardrails:** creation-start volume (`lp_community_started`) and the
  simulated submit-failure path (`lp_community_submit_failed`) must stay healthy.
- **Events:** `lp_community_gate_viewed` (entering the gate),
  `lp_community_started` (passing the sign-in gate),
  `lp_community_step_completed` (`{from,to}` per forward NEXT),
  `lp_community_review_reached` (entering review),
  `lp_community_submit_attempted` (review -> submitting),
  `lp_community_submit_failed` (`{error}`, simulated error -> back to review),
  `lp_community_created` (stub, reaching created).

Data reality: `catalyrst-communities` exists in the workspace but is **not
exposed on the live catalyst** (`/v1/communities` 404s), and the real create
needs a DCL auth-chain signature plus a held NAME, so the final create is
**SIMULATED** via an XState actor -- flow, states, field validation and metrics
are real; only the on-chain/HTTP commit is stubbed. Noted as deferred.
