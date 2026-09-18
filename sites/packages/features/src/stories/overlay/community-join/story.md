---
id: bevy-overlay-community-join
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided communities flow -- browse, open a community, then an explicit
    JOIN (public) or REQUEST TO JOIN (private) confirm step -- increases the
    share of community views that reach an acknowledged membership join or
    pending private-community request.
  because: >-
    Making the join path legible (what kind of community this is, who is in it,
    and exactly what the button will do before it does it) reduces hesitation,
    so more people who open a community follow through instead of bouncing at an
    ambiguous one-shot button.
metric:
  primary: cl_community_join_rate
  numerator: cl_community_joined
  denominator: cl_community_detail_viewed
  guardrails:
    - cl_community_browse_viewed
    - cl_community_request_submitted
experiment:
  key: cl_community_join
  unit: session
  variants:
    - id: guided
      weight: 1
      flags:
        wizard: true
  baseline: 0.25
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if cl_community_join_rate improves by at least the MDE with no
    guardrail regression (browse volume holds and private REQUEST TO JOIN
    submissions stay healthy); otherwise hold.
---

# Join a public community or request private membership

## Current capability

The production route injects `buildCommunityJoinCommit`, which resolves the
current identity for each action and uses signed POST requests to
`/v1/communities/{id}/members` or `/v1/communities/{id}/requests`.
Signed-out users receive a sign-in error; the route never substitutes a
simulated membership. Non-success responses remain errors.

Public joins and private requests have different outcomes: a private request is
pending and grants no member role. Request payloads use
`{ type: "request_to_join" }`. The backend owns authorization and admission.
The standalone machine still has a preview actor; its output is not proof of
shared membership. A failed browse request currently becomes an empty list,
which remains a limitation rather than evidence that no communities exist.

## Assumptions and measurement

Wallet connection does not imply community membership. Count joined members and
pending requests separately. `cl_community_join_rate` is `cl_community_joined`
over `cl_community_detail_viewed`; private request volume is a separate guardrail.
This single-arm draft cannot establish lift over a control.
Use the shared [Overlay assumptions](../../../../../../docs/product-capabilities.md#avatar-and-social-overlay).
