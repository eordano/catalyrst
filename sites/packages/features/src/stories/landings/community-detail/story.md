---
id: landings-community-detail
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A public community landing at /social/communities/:id that answers "who is
    this, who's in it, and what's happening" in one glance -- info header,
    members rail, and upcoming-events grid -- converts more anonymous visitors
    into JOIN / REQUEST TO JOIN actions than a bare card link.
  because: >-
    Communities are the social anchor of Decentraland; a trustworthy public
    profile that shows membership scale, the owner, and live/upcoming events
    gives a visitor enough signal to commit, while a private community's gated
    teaser sets the right expectation (SIGN IN / REQUEST) instead of a dead end.
metric:
  primary: lp_community_join_intent_rate
  guardrails:
    - lp_community_viewed
    - lp_community_tab_changed
    - lp_community_private_gated
experiment:
  key: lp_community_detail
  unit: session
  variants:
    - id: full_profile
      weight: 1
      flags:
        showMembersRail: true
        showEventsGrid: true
  baseline: 0.18
  mde: 0.04
decision:
  rule: >-
    Ship the info + members + events landing if lp_community_join_intent_rate
    (lp_community_join_intent / lp_community_viewed) clears the 0.18 baseline by
    the 0.04 MDE with 95% confidence and no guardrail regresses (views,
    tab-change engagement, private-gate rate); otherwise revise the CTA / rail
    prominence.
---

# Story -- Public community landing (`/social/communities/:id`)

A SIMPLE browse/detail surface (no XState -- there is no multi-step on-chain
flow here; JOIN / REQUEST TO JOIN is a single identity-gated CTA owned by the
client, surfaced presentationally). The route is
`app/routes/landings.community-detail.tsx`; the journey steps are URL-addressable
via `?id` and `?tab` (and `?gate=private` to force the private teaser for QA).

## Data

`dataSource: upstream`. The `catalyrst-communities` crate
(`crates/catalyrst-communities`, `src/lib.rs` `api_router`) defines
`GET /v1/communities/{id}` / `/members` / `/posts`, and those routes are now
proxied on live `catalyst.example.com` (probed -> 200).
`app/lib/catalyst/overlay/communities.ts` is wired **live-only**: with no
explicit `?id=`, the loader picks the most-populated real community from
`GET /v1/communities` (`loadDefaultCommunity`); on any failure or when the
service has no communities it renders the honest not-found state -- never
fixture data. Field names and the privacy/visibility/role enums are
cross-checked against `decentraland/social-service-ea`
`src/logic/community/types.ts` (`CommunityPrivacyEnum`,
`CommunityVisibilityEnum`, `CommunityRole`,
`AggregatedCommunityWithMemberAndVoiceChatData`).

## Journey steps & events

- **view / info** -- `/social/communities/:id`. Renders ui3
  `StSocialCommunityDetail` (info header: thumbnail, label, name, privacy +
  members count, owner, JOIN/REQUEST/SIGN IN CTA). Fires `lp_community_viewed`
  `{ community_id, privacy, members_count, source }` once on load.
- **members-tab** -- `?tab=members` (default). The members rail; selecting it
  fires `lp_community_tab_changed` `{ community_id, tab: "members" }`.
- **events-tab** -- `?tab=events`. The upcoming-events grid; selecting it fires
  `lp_community_tab_changed` `{ community_id, tab: "events" }`.
- **private-gate** -- a `private` community the viewer can't see renders the
  PrivateMessage teaser instead of the members/events sections and fires
  `lp_community_private_gated` `{ community_id }`. Forceable via `?gate=private`.
- The JOIN / REQUEST TO JOIN / SIGN IN CTA emits `lp_community_join_intent`
  `{ community_id, privacy, intent }` -- the numerator of the primary metric.
  (The membership write itself is owned by the authenticated social client and
  is **out of scope / not simulated** here; this landing captures intent only.)

Primary metric
`lp_community_join_intent_rate = lp_community_join_intent / lp_community_viewed`.
