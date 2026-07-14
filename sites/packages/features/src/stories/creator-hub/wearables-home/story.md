---
id: creator-wearables-home
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Bringing the wearables/emotes Collections home INTO the Creator Hub -- a
    server-rendered grid of the creator's collections and single items in the
    familiar desktop chrome, instead of the "coming soon" placeholder that
    bounces them out to the Builder -- raises the share of Creator Hub wearables
    visits that open a collection or single item, keeping creators inside the
    app to start their next publish.
  because: >-
    Today the Creator Hub Collections entry is a disabled "coming soon" card
    (ChCollections) that links out to builder.decentraland.org, so creators
    leave the app to see what they own. Rendering the real collections + single
    items home in-app, with grid/list views split across Collections / Linked
    Wearables / Single items, makes the next step (open a card) obvious -- so a
    higher share of home views produce a card click rather than a bounce, and an
    empty wallet is funnelled straight into Create.
metric:
  primary: creator_wearables_card_clicked_rate
  guardrails:
    - creator_wearables_home_viewed
    - creator_wearables_view_changed
experiment:
  key: creator_wearables_home
  unit: session
  variants:
    - id: wearables-home
      weight: 1
      flags:
        showWearablesHome: true
  baseline: 0.31
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if creator_wearables_card_clicked_rate (card-click sessions /
    creator_wearables_home_viewed sessions) improves by at least the MDE with no
    regression in creator_wearables_home_viewed volume and the empty-wallet
    Create path stays graceful; otherwise iterate on the card/tab layout.
---

# Creator Hub Wearables home: browse collections

The wearables/emotes Collections home, brought into the Creator Hub desktop
shell (`CreatorHubChrome`) instead of the `ChCollections` "coming soon"
placeholder that links out to the Builder. The loader lists the creator's
collections -- standard and third-party (tagged "Linked") -- live from the
builder gateway (`loadCollections` in `builder/collections.server.ts`) and
returns plain data so the page server-renders without JS. ui3's
`WearablesHomeView` renders the status-dotted card grid with a grid/list
toggle; signed-out and empty wallets get an EmptyState funnelling into Create.

Single shipping variant (`wearables-home`).

- **Primary metric:** `creator_wearables_card_clicked_rate` =
  `creator_wearables_card_clicked` / `creator_wearables_home_viewed`.
- **Guardrails:** `creator_wearables_home_viewed` volume, plus the grid/list
  interaction (`creator_wearables_view_changed`) must not regress.
- **Events:** `creator_wearables_home_viewed` on load (`{ count }`);
  `creator_wearables_card_clicked` on a collection card click (`{ id, kind }`,
  kind `collection` | `third_party`); `creator_wearables_view_changed` on the
  grid/list toggle (`{ view }`); `creator_wearables_signin_clicked` on the
  signed-out CTA.

Journey steps (URL-addressable):

1. Open `/create/wearables` -- `creator_wearables_home_viewed { count }`.
2. Toggle grid/list via `?view=list` -- `creator_wearables_view_changed { view }`.
3. Click a collection card -> `/create/wearables/collections/{id}` --
   `creator_wearables_card_clicked { id, kind }`.
4. Empty / unknown wallet (`?address=0x0`) -> graceful "No collections yet"
   empty state with a Create CTA (an upstream 404 counts as an empty wallet,
   not an outage).
5. Gateway outage -> honest error banner with a Retry (revalidate) button --
   no fixture rows are ever rendered.

Data reality: the loader reads the live builder gateway only
(`fetchCollections`, Zod-validated). There is **no fixture fallback** -- a
failed read renders the error banner + retry, an empty read the empty state.
The earlier tabbed layout (Collections / Linked Wearables / Single items) was
collapsed into a single card grid: linked collections render inline tagged
"Linked", and single (orphan) items have no home surface yet
(`fetchOrphanItems` exists in the lib but nothing renders it -- an open gap).
Wearable thumbnail renders are remote in the real product -> gradient-washed
placeholders. Nothing is on-chain here (browse-only).
