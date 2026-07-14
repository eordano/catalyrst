---
id: landings-profile
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A single public profile landing at /profile/:address that surfaces a
    member's identity (avatar, name, badges, about, links, equipped items)
    alongside deep-linkable tabs -- Overview, Creations, Places, Photos,
    Communities and (own profile) My Assets -- lets visitors evaluate a member at
    a glance and explore what they make, host and own, raising the share of
    profile sessions that open a second tab versus a single Overview-only view.
  because: >-
    Visitors arrive at a profile to judge who someone is and what they have
    built; co-locating the avatar overview with one-click, URL-addressable tabs
    for creations / places / photos / communities cuts the navigation cost of
    discovering a member's footprint and lifts the profile-to-explore rate.
metric:
  primary: profile_viewed -> profile_tab_changed conversion
  guardrails:
    - profile_tab_changed
    - profile_card_clicked
experiment:
  key: landings_profile
  unit: session
  variants:
    - id: default
      weight: 100
      flags: {}
  baseline: 0.41
  mde: 0.05
decision:
  rule: >-
    Ship the unified tabbed profile landing if the tab-engagement rate
    (profile_tab_changed sessions / profile_viewed sessions) beats the
    Overview-only baseline with 95% confidence; otherwise iterate on the default
    tab and the order of the Creations / Places / Photos tabs.
---

# Landings -- Public profile landing (/profile/:address)

The visitor opens a public member profile at `/profile/:address`. The page
attempts the LIVE catalyst lambdas profile entity
(`GET https://catalyst.example.com/lambdas/profile/{address}` ->
`{ avatars: Avatar[], timestamp }`, the deployed Profile/Avatar shape from
decentraland/schemas `src/platform/profile/{profile,avatar}.ts`) and, when that
is empty/unreachable, falls back to the bundled fixture
(`app/fixtures/landings-profile.json`). The profile identity + each tab compose
the ui3 `StProfileOverviewTab` / `StProfileCreationsTab` / `StProfilePlacesTab` /
`StProfilePhotosTab` / `StProfileCommunitiesTab` / `StProfileMyAssetsTab` screens
under `SitesChrome`. Every tab is URL-addressable via `?tab=`.

Journey + metrics:

- `/profile/:address` (overview) loads the SSR profile card + Overview tab ->
  `profile_viewed { address, has_claimed_name, source, own }`.
- `?tab=creations` / `?tab=places` / `?tab=photos` / `?tab=communities` /
  `?tab=assets` switch tabs -> `profile_tab_changed { tab }`.
- Selecting a place / photo / community / equipped item card (the per-tab
  detail openers) -> `profile_card_clicked { tab, item_id }`.

Tab visibility mirrors the real `ProfileTabs.getVisibleTabs`: a member profile
shows Overview / Creations / Communities / Places / Photos; the OWN profile
(`?own=1`) swaps to Overview / My Assets / My Communities / My Places /
My Photos / Referral Rewards (so the `my-assets-tab` step is reachable).

Data source: live catalyst lambdas Profile, with a fixture fallback. The Avatar
identity (name, hasClaimedName, nameColor, description/bio, links, info fields)
is projected by `lib/catalyst/profile.ts mapProfile()`; the per-tab content
(creations wearables/emotes, places, photos, communities, owned assets) is
served from the fixture because those collections come from other backends
(explorer-api wearables-by-owner, catalyrst-places, catalyrst-camera-reel, the
social-service communities API) which return empty at this gateway.

Deferred / simulated:

- The LIVE lambdas profile returns `{ avatars: [], timestamp: 0 }` at this
  gateway, so in practice the page renders the fixture
  identity; the live path is wired and used whenever a deployed profile exists.
- Creations / Places / Photos / Communities / Assets collections are served
  from the fixture (DERIVED from decentraland/schemas + the ui3 component prop
  contracts) -- the explorer-api / camera-reel / places / communities reads for
  these tabs return empty at this gateway. A real deployment would back each tab
  with its own catalyst read; the flow, tab routing and metrics are real.
- "Add friend" / "Copy address" / friend-graph mutual-count are interactive
  affordances inside the ui3 screens and are not wired to a backend here.
