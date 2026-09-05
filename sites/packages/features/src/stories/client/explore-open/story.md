---
id: client-explore-open
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing the live places explorer inside the HUD lets players discover and
    jump to destinations without leaving the world, raising the share of sessions
    that open the explore panel and click through to a place.
  because: >-
    Embedding the live /places browse grid (server-rendered via loadPlaces) in a
    URL-addressable panel (?panel=explore) keeps discovery in-context; players who
    open it convert to a place-card click (place_card_clicked) that reuses the
    existing /places/:id jump-in flow.
metric:
  primary: cl_explore_open_rate
  numerator: cl_explore_opened
  guardrails:
    - place_card_clicked
experiment:
  key: client_explore_open
  unit: session
  variants:
    - id: hud-explore
      weight: 100
      flags:
        embedsPlaces: true
  baseline: 0.0
  mde: 0.05
decision:
  rule: >-
    Single-variant rollout. Ship if cl_explore_opened / cl_hud_loaded clears a
    healthy open-rate and place_card_clicked (the guardrail conversion to a
    destination click) holds; otherwise revisit the explore affordance.
---

# Client H03 -- Open the explore/map panel

From the loaded HUD, clicking Places/Map opens the explore panel at the
URL-addressable `/client?panel=explore`. The panel embeds the live places browse
grid (`loadPlaces`, server-rendered) -- the same Catalyst-backed surface as
`/places`. Place cards link to the existing `/places/:id` jump-in route so
discovery -> jump-in reuses the already-templated detail flow.

Journey:

1. `/client` (HUD loaded).
2. Click Places/Map in the Sidebar -> `/client?panel=explore` (URL-addressable).
3. ExplorePanel embeds the live `/places` browse grid (loadPlaces, SSR).
4. `cl_explore_opened` fires on panel open (with `place_count`).
5. Click a place card -> reuse `/places/:id` jump-in; `place_card_clicked` fires
   (guardrail).

Primary metric `cl_explore_open_rate` = `cl_explore_opened` / `cl_hud_loaded`,
derivable from the events above. The Places API is live, but PlaceRow omits some
live/featured fields (capability-matrix #6) -- the grid renders gracefully.
