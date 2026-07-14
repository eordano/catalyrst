---
id: creator-integration-create-entry
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Adding an explicit "Make a Wearable" entry card (plus the matching nav link)
    to the Creator Hub home increases the share of hub visitors who click into
    the wearable-creation flow, instead of the wearable path being buried behind
    the scenes-first surface.
  because: >-
    The hub home today is scenes-first; wearable and emote creators have no
    first-class entry point and must already know the /create/wearables route.
    Surfacing a prominent, legible "Make a Wearable" card with a direct CTA
    removes that discovery gap, so more hub sessions reach the item editor /
    create-collection flow instead of bouncing.
metric:
  primary: ch_create_wearable_click_rate
  guardrails:
    - ch_home_viewed
    - create_entry_viewed
experiment:
  key: ch_create_entry
  unit: session
  variants:
    - id: entry_cards
      weight: 1
      flags:
        createEntry: true
  baseline: 0.2
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if ch_create_wearable_click_rate improves by at least the MDE with no
    guardrail regression (hub viewing volume holds and the entry-card row keeps
    rendering for every session); otherwise hold.
---

# Add a Make-a-Wearable entry card to the Creator Hub home

The Creator Hub home (`/create`) is scenes-first: the only first-class card is
the Scenes card, so wearable and emote creators have no obvious way in. This
story adds a static **vertical create-entry card list** above the hub grid --
**Make a Wearable** (featured), **Animate an Emote**, and **Build a Scene** --
plus a matching nav link, and tracks whether that lifts the share of hub
visitors who click into the wearable-creation flow.

- **Primary metric:** `ch_create_wearable_click_rate` =
  `ch_create_wearable_clicked` / `ch_home_viewed`.
- **Guardrails:** total hub-view volume (`ch_home_viewed`) and the entry-row
  render (`ch_create_entry_viewed`) must stay healthy.
- **Events:** `ch_home_viewed` on hub load (shared with `create-hub-to-scenes`),
  `ch_create_entry_viewed` when the create-entry card row renders,
  **`ch_create_wearable_clicked`** `{ target, vertical }` on the Make-a-Wearable
  card CTA (and the matching nav link), `ch_create_entry_clicked`
  `{ vertical, target }` on the emote/scene cards, and `ch_create_learn_clicked`
  on the public create-landing (StCreate) link.

Data reality: the deployed-scenes read is LIVE (Catalyst Places API via
`loadCreatorScenes`); the create-entry **vertical card list is static** editorial
content lifted verbatim from decentraland.org/create
(decentraland/sites `components/Create/data.ts`, mirrored by ui3 `StCreate`) and
captured in `fixtures/creator-integration-create-entry.json` (`_source`). The
make-a-wearable CTA deep-links into the existing item-editor / create-collection
flows (`/create/wearables/item-editor`, `/create/wearables/collections/new`);
those downstream commits remain SIMULATED in their own stories. Nothing on-chain
is signed here -- this story only adds the entry surface + its click telemetry.
