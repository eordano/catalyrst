---
id: creator-hub-manage-worlds
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Giving creators a single "Manage Worlds" hub that lists every World/LAND
    they have deployed to -- each as a PublishedProjectCard with its scene count
    and role chip -- alongside a persistent storage-quota ring (used / max MB
    earned from MANA, LAND and NAME holdings), Published / Not published filter
    chips, sort, and search, makes it obvious how much World storage is left and
    which domain to manage next, lifting the share of Manage views that act on a
    World (open settings, view scenes, or open the storage breakdown).
  because: >-
    Creators arrive at Manage to check what they have shipped and whether they
    have room to ship more. Surfacing every deployed World as a card with a
    visible scene count, plus an always-on storage ring and a filter to separate
    published from not-yet-published NAMEs, removes the guesswork about quota and
    deployment status -- so a higher share of Manage views produce a card or
    storage interaction, and an empty creator is funnelled straight into Mint a
    NAME / View Scenes.
metric:
  primary: ch_manage_card_clicked_rate
  guardrails:
    - ch_manage_viewed
    - ch_manage_filter_changed
    - ch_manage_storage_opened
    - ch_manage_empty_viewed
experiment:
  key: creator-hub-manage-worlds
  unit: session
  baseline: 0.41
  mde: 0.05
  min_sample: 3000
  variants:
    - id: manage-hub
      weight: 1
      flags:
        showManageHub: true
decision:
  rule: >-
    Ship if ch_manage_card_clicked_rate (card-click sessions / ch_manage_viewed
    sessions) improves by at least the MDE with no regression in ch_manage_viewed
    volume and the empty-creator Mint-a-NAME / View-Scenes path stays graceful;
    otherwise iterate on the card layout, the filter/sort placement, and the
    storage-ring affordance.
---

# Manage Worlds: list every deployed scene with the storage-quota sidebar

The Creator Hub "Manage Worlds" screen (creator-hub
`renderer/src/components/ManagePage/component.tsx`), composed here inside ui3's
`CreatorHubChrome` desktop shell. The loader lists every World the wallet has
deployed to + the NAMEs that gate Worlds, and returns plain data so the page
server-renders without JS. The route renders the PublishedProjectCard grid
itself (over ui3's `chmanage.css` layer); ui3's `EmptyState` backs the empty
path; the storage breakdown lives on the Worlds-storage dashboard and is
reached from the "Your Storage" affordance
(`/creator-hub/worlds-storage?quota=1`).

Single shipping variant (`manage-hub`). This is a browse/manage surface
(loader + components), NOT a multi-step write flow, so there is no XState
wizard. The on-chain actions a card exposes (open World Settings, deploy a new
scene, Mint a NAME, Buy MANA/LAND/NAME) are real navigations whose destinations
are owned by their own stories -- they are SIMULATED here as links/handlers.

- **Primary metric:** `ch_manage_card_clicked_rate` =
  `ch_manage_card_clicked` / `ch_manage_viewed`.
- **Guardrails:** `ch_manage_viewed` volume, plus the filter, storage-modal, and
  empty-state events must not regress.
- **Events:**
  - `ch_manage_viewed` on load (`{ count, filter, sort, search, address }`).
  - `ch_manage_filter_changed` on a Published / Not published chip
    (`{ filter }`) -> `?filter=`.
  - `ch_manage_sorted` on a sort change (`{ sort }`, published filter only)
    -> `?sort=`.
  - `ch_manage_searched` on a search submit (`{ q }`) -> `?search=`.
  - `ch_manage_card_clicked` on a World card (`{ id, role }`).
  - `ch_manage_storage_opened` on the "Your Storage" affordance (`{}`) --
    navigates to the storage dashboard's quota panel
    (`/creator-hub/worlds-storage?quota=1`).
  - `ch_manage_empty_viewed` when an empty list renders (`{ filter }`).

Journey steps (URL-addressable):

1. Open `/creator-hub/manage` -- `ch_manage_viewed { count, filter, sort }`.
2. Filter via `?filter=unpublished` (Published / Not published chips) --
   `ch_manage_filter_changed { filter }`.
3. Sort via `?sort=domain` (Last published / Domain name; published filter only)
   -- `ch_manage_sorted { sort }`.
4. Search via `?search=neon` (filter by domain/name) --
   `ch_manage_searched { q }`.
5. Empty state via `?address=0x0` (or `?filter=unpublished` with no matches) --
   the Mint-a-new-NAME / View-Scenes empty state, `ch_manage_empty_viewed`.
6. Storage breakdown via the "Your Storage" affordance --
   `ch_manage_storage_opened`, landing on the Worlds-storage dashboard's quota
   panel (`/creator-hub/worlds-storage?quota=1`).

Data reality: the World **list** is LIVE -- the wallet's NAMEs come from the
Lambdas (`/lambdas/users/{address}/names`) and each NAME's deployment status +
scene count from the worlds-content-server `/world/{name}/about` (`scenesUrn`).
There is **no fixture fallback**: when the Worlds service is unreachable the
route shows an honest error banner with a retry and an empty list rather than
sample data. The **storage ring/quota** is NOT rendered on this page --
worlds-content-server's `GET /wallet/:wallet/stats` is not exposed on this
realm (404), so instead of fabricating a quota the page links to the
Worlds-storage dashboard, which owns the honest quota view.
