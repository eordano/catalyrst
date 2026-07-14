---
id: creator-integration-redirect-collections
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Redirecting the legacy Builder collection entry points
    (/builder/collections and /builder/create-collection) to the unified
    Creator Hub wearables area (/create/wearables), with the query string
    preserved, lands creators on a single canonical surface without dead ends
    or duplicate, divergent collection UIs.
  because: >-
    Creators arriving via old Builder links or bookmarks should reach the same
    place as creators who start in the Creator Hub. Forwarding them (302) rather
    than maintaining a second, separate collections screen removes a confusing
    fork in the funnel, so a higher share of legacy-link arrivals continue into
    the Creator Hub instead of bouncing on an orphaned page.
metric:
  primary: creator_builder_redirect
  guardrails:
    - creator_builder_redirect
experiment:
  key: creator_builder_redirect_collections
  unit: session
  baseline: 0.0
  mde: 0.05
  min_sample: 2000
  variants:
    - id: redirect
      weight: 1
      flags:
        redirect: true
        target: /create/wearables
decision:
  rule: >-
    Ship the redirect if legacy-link arrivals (creator_builder_redirect) are
    forwarded to /create/wearables with no increase in bounces on the Creator
    Hub landing and the query string is preserved on arrival; otherwise keep the
    legacy pages live while the Creator Hub surface stabilizes.
---

# Redirect legacy Builder collection URLs to the Creator Hub

The legacy Builder collection entry points are consolidated into the unified
Creator Hub wearables area. Two loader-only routes issue a server `redirect()`
(HTTP 302) and forward the incoming query string verbatim:

- `/builder/collections` -> `/create/wearables`
- `/builder/create-collection` -> `/create/wearables`
  (the interactive create-collection wizard itself lives at
  `/create/wearables/collections/new`).

Each route is loader-only: there is no component, no UI, and no live Catalyst
read (`dataSource: none`). The loader mints/persists the `sid` cookie (so the
arrival is attributable across the redirect), emits the redirect event, and
returns a 302 `Response` whose `Location` is `/create/wearables` plus the
original `?...` query string.

- **Primary metric:** `creator_builder_redirect` -- emitted once per redirect,
  tagged with `{ from, to, query }` so we can see which legacy entry point sent
  the traffic and whether the query survived.
- **Events:** `creator_builder_redirect` (`{ from, to, query }`) on every
  forward.

Data reality: nothing is fetched -- the redirect map is documented in
`app/fixtures/creator-integration-redirect-collections.json` (with `_source`).
The forward target `/create/wearables` is the canonical Creator Hub wearables
area; the destination page itself is owned by the Creator Hub stories. No
on-chain or LiveKit work is involved. Nothing is simulated beyond the redirect.
