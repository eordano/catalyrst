---
id: creator-integration-redirect-item-publish-curate
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Permanently redirecting the legacy /builder creator surfaces (item editor,
    publish-collection, curation, collection/:id, item/:id) into their unified
    /create homes -- preserving the incoming query and path params -- lets a
    creator who follows an old bookmark, deep link or in-app link land on the
    exact equivalent screen in the unified Creator Hub instead of a dead or
    duplicate page.
  because: >-
    The Creator Hub is being consolidated under /create; keeping two parallel
    surfaces (/builder and /create) splits traffic, telemetry and maintenance.
    A loss-less 308 redirect (?collection / ?item / ?step / filters carried
    through, :id folded into the editor's query) means existing links keep
    working and every creator funnels into the single unified hub, so legacy
    deep links convert to the unified screen instead of bouncing.
metric:
  primary: creator_builder_redirect
  guardrails:
    - creator_builder_redirect_dashboard_viewed
experiment:
  key: creator_builder_redirect
  unit: session
  baseline: 0
  mde: 0.05
  min_sample: 1000
  variants:
    - id: redirect
      weight: 1
      flags:
        redirect: true
        status: 308
decision:
  rule: >-
    Ship the redirects unconditionally (this is a migration, not an A/B test):
    the single `redirect` arm is always served. Monitor creator_builder_redirect
    volume per legacy surface to decide when each legacy /builder entry point is
    cold enough to drop from navigation; hold the redirect in place until volume
    approaches zero.
---

# Redirect legacy /builder creator surfaces into the unified /create hub

Part of the **CREATOR HUB unification** theme. The wearable creation surfaces
that historically lived on `builder.decentraland.org` now live in the unified
Creator Hub under `/create`. This story turns the five legacy `/builder/*`
routes into **loader-only 308 redirects** into their `/create` equivalents,
**preserving the incoming query string and path params**, and emits a single
`creator_builder_redirect` event so the migration of each entry point is
observable.

No data is fetched (`dataSource: none`) -- each route maps its URL onto the
canonical `/create` target and redirects. The mapping is the single source of
truth in `app/lib/creator/builder-redirect.ts` (shared by the routes and the
dashboard):

| Legacy surface | Unified destination | Params preserved |
| --- | --- | --- |
| `/builder/item-editor` | `/create/wearables/item-editor` | `?collection ?item ?step ?variant` |
| `/builder/publish-collection` | `/create/wearables/publish` | `?collection ?step ?variant` |
| `/builder/curation` | `/create/curate` | `?step ?id ?decision ?status ?type ?assignee ?committee ?variant` |
| `/builder/collection/:id` | `/create/wearables/collections/:id` | `:id` &rarr; path, `?tab ?variant` |
| `/builder/item/:id` | `/create/wearables/item-editor` | `:id` &rarr; `?item`, `?variant` |

The `/create` collection **detail** page has shipped
(`/create/wearables/collections/:id`), so the legacy `collection/:id` deep link
lands on the real detail surface with its `:id` carried in the path. `item/:id`
still folds into the unified item editor's `?item` query: the
`/create/wearables/items/:id` detail page resolves marketplace catalog ids
(`{contract}-{itemId}`), while legacy `/builder/item/:id` ids are builder-server
UUIDs the catalog can't resolve -- the editor accepts any `?item` and opens
focused on it. The rest of the query rides through unchanged.

- **Primary metric:** `creator_builder_redirect` -- emitted once per redirect with
  `{ from, to, fromPath }`. Volume per `from` surface measures which legacy entry
  points are still trafficked.
- **Guardrail:** `creator_builder_redirect_dashboard_viewed` -- the migration
  dashboard is reachable and rendering the rollup.
- **Events:**
  - `creator_builder_redirect` -- fired in every redirect loader
    (`{ from, to, fromPath }`),
  - `creator_builder_redirect_dashboard_viewed` -- on the dashboard mount
    (`{ window_days, total }`).

## Journey steps (URL-addressable)

1. Hit a legacy item-editor deep link `/builder/item-editor?collection=demo&step=model`
   &rarr; 308 to `/create/wearables/item-editor?collection=demo&step=model`
   (`creator_builder_redirect { from: "item-editor" }`).
2. Hit legacy publish `/builder/publish-collection?collection=demo`
   &rarr; 308 to `/create/wearables/publish?collection=demo`.
3. Hit legacy curation `/builder/curation?status=to_review&step=review`
   &rarr; 308 to `/create/curate?status=to_review&step=review`.
4. Hit a legacy collection-detail deep link `/builder/collection/demo-collection?tab=emotes`
   &rarr; 308 to `/create/wearables/collections/demo-collection?tab=emotes`
   (the `:id` is carried in the destination path).
5. Hit a legacy item-detail deep link `/builder/item/demo-item-1`
   &rarr; 308 to `/create/wearables/item-editor?item=demo-item-1`.
6. Open the migration dashboard `/builder/redirects-metrics` -- the
   legacy&rarr;unified mapping table + captured 7-day redirect volume per surface
   (`creator_builder_redirect_dashboard_viewed`).

## Data source

`dataSource: none`. The redirects perform no fetch. The dashboard renders the
static mapping contract in
`app/lib/catalyst/creator-hub/builder-redirect.ts` (`REDIRECT_DASHBOARD`) --
there is no SSR read-back path for the emitted event, so **no volume numbers are
shown** (the dashboard says so; the `creator_builder_redirect` events themselves
are real, fired server-side in each redirect loader). The `/create` destination
routes (`create.wearables.item-editor`, `create.wearables.publish`,
`create.curate`, `create.wearables.collections_.$id`) are the real unified
surfaces in this repo.

## Deferred / simulated

- **Per-surface counts** on the dashboard are unavailable (no SSR read-back path
  for client/server events) -- the table shows the mapping contract only.
- **`item/:id`** still redirects to the item editor (not the
  `/create/wearables/items/:id` detail page) because legacy builder item ids are
  builder-server UUIDs the marketplace-catalog-backed detail page cannot
  resolve; repoint the `item-detail` rule in `lib/creator/builder-redirect.ts`
  if the detail page ever learns to resolve builder ids.
