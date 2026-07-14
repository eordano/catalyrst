---
id: bevy-overlay-community-join
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided communities flow -- browse, open a community, then an explicit
    JOIN (public) or REQUEST TO JOIN (private) confirm step -- increases the
    share of community views that reach a membership commit, even with the
    commit simulated.
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

# Browse communities and join (or request to join) one

The bevy-overlay Communities surface (the in-client `[O]` explore tab) lets a
player browse communities, open one to see its members / visibility / live
stream, and then commit: a **Public** community shows a **JOIN** button (POST
`/v1/communities/{id}/members`, `CommunityJoin`); a **Private** community shows
**REQUEST TO JOIN** (POST `/v1/communities/{id}/requests`, `CommunityRequest`
`{ kind: "request_to_join" }`). This story tracks whether the guided flow lifts
the share of community views that reach a membership commit.

- **Primary metric:** `cl_community_join_rate` =
  `cl_community_joined` / `cl_community_detail_viewed`.
- **Guardrails:** browse volume (`cl_community_browse_viewed`) and private
  request submissions (`cl_community_request_submitted`) must stay healthy.
- **Events:** `cl_community_browse_viewed` (browser open, `{ count, search }`),
  `cl_community_detail_viewed` (`{ community_id, privacy, members_count }`),
  `cl_community_join_started` (`{ community_id, action }`),
  `cl_community_request_submitted` (private, `{ community_id }`),
  `cl_community_joined` (SIMULATED commit, `{ community_id, action, pending, stub }`).

## Data reality (deferred / simulated)

The `catalyrst-communities` crate implements the full surface -- `get_communities`
(browse), `get_community` + `get_members` (detail), `writes::add_member`
(`CommunityJoin`) and `writes::create_request` (`CommunityRequest`) -- and
`GET /v1/communities` is now routed on `catalyst.example.com` (probed ->
200). The loader is wired **live-only**: on failure the route renders the
honest empty state ("No communities found."), never fixture data.

The join / request commit is **SIMULATED**: both write routes call
`require_signer()` and demand a DCL auth-chain signature an anonymous browser
session does not have (fail-closed). The XState `communityJoinMachine` runs a
simulated commit (`simulateCommit`) -- the flow, states, telemetry, and the
public-vs-private branch are all real; only the final network write is stubbed,
and clearly labelled as such in the UI and the `cl_community_joined` event
(`stub: true`).
