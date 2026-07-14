# Metrics taxonomy & Builder -> Creator-Hub integration
Status: design / "start thinking about metrics"
Owner: owner@example.com
Scope: CREATOR, SCENE-OPERATOR, ADMIN, CAST personas across the unified Creator Hub.

Canonical analytics taxonomy: funnel -> route mapping per persona, plus the plan to fold the standalone Builder screens into one Creator Hub. Source of truth for the per-persona dashboard instrumentation stories.
## 1. Telemetry (recap, so names stay consistent)
All analytics go through one helper: `packages/core/src/lib/telemetry/track.ts`.

- `track(event, props, ctx)` - best-effort send to catalyrst-telemetry (`POST {TELEMETRY_URL}/v1/track`, Segment `track` shape; `/events` on that service is a GET-only SSR dashboard page, not an ingest sink). Isomorphic, fire-and-forget, MUST NEVER throw.
- `trackExposure(ctx)` - emits the canonical `experiment_exposed` event exactly once when an experiment surface renders. Do NOT double-fire it on deep-link `?step` hydration.
- `ctx: TrackContext = { sid, story?, variant?, experimentKey? }` - `sid` from `ensureSid(request)` (`packages/core/src/lib/experiments/assign.ts`); `story`/`variant`/`experimentKey` from `resolveAssignment` + `parseStory`. Each event carries `story`, `variant`, `exp_key` so Metabase readouts can segment by arm.
### Naming convention (canonical)
`<persona>_<funnel>_<action>`, snake_case, persona-prefixed. Existing per-surface prefixes (`ch_*` Creator Hub, `bd_*` Builder, `mk_*` Marketplace, `gv_*` Governance, `cast_*` Cast) remain valid: the persona prefix is the analytics rollup key, the surface prefix is the emitting screen.

| Persona        | Prefix      | Surfaces (emitting screens)                                  |
| -------------- | ----------- | ------------------------------------------------------------ |
| CREATOR        | `creator_*` | Creator Hub `ch_*`, Builder collections/items `bd_*`, Cast publish |
| SCENE-OPERATOR | `operator_*`| Creator Hub deploy/worlds `ch_*`, Builder scenes/worlds `bd_*`, scene-admin |
| ADMIN          | `admin_*`   | What's-On admin, Builder curation `bd_*`, governance/community moderation |
| CAST           | `cast_*`    | Cast streamer console + watcher                              |

Funnel rollup events use the persona prefix (e.g. `creator_publish_step_viewed`); fine-grained surface interactions keep their screen prefix (e.g. `bd_collections_card_clicked`). Dashboards read the persona rollups; screen events stay for drill-down.
Properties on every funnel event (in addition to `ctx`): `step`, `funnel`, `address` (creator/operator wallet, when known), `surface` (route/screen id). Money/volume events add `mana`, `usd`, `count`.
## 2. CREATOR persona - publish / sales / collection funnel
**F-CRE-1 Collection publish (curation) funnel** - build collection -> add items -> submit for curation -> approved -> on sale.

| Step              | Event                              | Emitting route(s) / surface                                  |
| ----------------- | ---------------------------------- | ------------------------------------------------------------ |
| collection viewed | `creator_collection_viewed`        | `packages/routes/app/routes/builder.collections.tsx`, `builder.collection_.$id.tsx` |
| item added/edited | `creator_item_edited`              | `packages/routes/app/routes/builder.item-editor.tsx`                          |
| publish started   | `creator_publish_started`          | `packages/routes/app/routes/builder.publish-collection.tsx`                   |
| curation submitted| `creator_curation_submitted`       | `packages/routes/app/routes/builder.publish-collection.tsx` (commit SIMULATED)|
| published / on sale| `creator_collection_published`    | `packages/routes/app/routes/builder.publish-collection.tsx` (success step)    |

- Primary metric: `creator_collection_published` rate (published / publish_started).
- Guardrails: `creator_publish_started` (volume holds), `creator_curation_submitted` (no drop at the curation gate).

**F-CRE-2 Item sale funnel** - listed -> viewed -> bought (mints/sales realized).

| Step          | Event                          | Emitting route(s)                                   |
| ------------- | ------------------------------ | --------------------------------------------------- |
| listed        | `creator_item_listed`          | `packages/routes/app/routes/marketplace.sell.tsx`                   |
| store viewed  | `creator_store_viewed`         | `packages/routes/app/routes/marketplace.collection.tsx`             |
| sale realized | `creator_sale_completed`       | `packages/routes/app/routes/marketplace.buy.tsx` (on-chain SIMULATED)|

- Primary metric: `creator_sale_completed` count / GMV (`mana`, `usd` props).
- Guardrails: `creator_item_listed` (supply), `creator_store_viewed` (demand).

**F-CRE-3 Scene publish funnel** (shared with operator; rolled up under creator for "did the creator ship content"): see F-OP-1.
### CREATOR dashboard
New route `packages/routes/app/routes/creator-hub.metrics.tsx` (Creator Hub Chrome, new "Metrics" tab): published collections, on-sale items, 7d sales. Emits `creator_dashboard_viewed` + `creator_dashboard_funnel_clicked` ({ funnel }). Data: builder-server collections/items + marketplace catalog (`packages/data/src/lib/catalyst/builder-collections.ts`, `marketplace` libs); fixture fallback.
## 3. SCENE-OPERATOR persona - scene-admin / bans / visits / deploys
**F-OP-1 Deploy funnel (LAND + World)** - choose target -> place/validate -> review -> deploy -> live.

| Step              | Event                          | Emitting route(s)                                  |
| ----------------- | ------------------------------ | -------------------------------------------------- |
| deploy started    | `operator_deploy_started`      | `creator-hub.deploy-land.tsx`, `creator-hub.deploy-world.tsx` |
| placement validated| `operator_placement_validated`| `creator-hub.deploy-land.tsx`                       |
| placement rejected| `operator_placement_rejected`  | `creator-hub.deploy-land.tsx` (guardrail)           |
| deployed          | `operator_deploy_completed`    | both deploy routes (signed `POST /content/entities` SIMULATED) |

- Primary metric: `operator_deploy_completed` rate (mirrors existing `ch_deploy_land_deploy_rate`).
- Guardrails: `operator_deploy_started`, `operator_placement_rejected`.

**F-OP-2 Scene-admin / moderation funnel** - open scene-admin -> grant/revoke admin -> ban/unban user.

| Step           | Event                       | Emitting route(s) / data                                  |
| -------------- | --------------------------- | --------------------------------------------------------- |
| admin opened   | `operator_scene_admin_opened`| new `creator-hub.scene-admin.tsx` (catalyst comms `/scene-admin`) |
| admin changed  | `operator_admin_changed`    | same ({ action: grant\|revoke })                           |
| ban issued     | `operator_ban_issued`       | same (catalyst comms `/users/{addr}/bans`, commit SIMULATED) |

- Primary metric: time-to-moderate proxied by `operator_ban_issued` per session.
- Guardrails: `operator_scene_admin_opened` (reach), ban reversal rate.

**F-OP-3 Visits / occupancy** (passive health metric, not a funnel)

| Event                      | Emitting route(s) / data                                |
| -------------------------- | ------------------------------------------------------- |
| `operator_visits_viewed`   | the operator dashboard, on load                          |

- Data source: catalyrst-presence (`GET /current`, `/current/scenes`, `/current/worlds`, `/scenes/history`) - `peers_count`, per-scene `count`, per-world headcount. REAL live data.
### SCENE-OPERATOR dashboard
New route `packages/routes/app/routes/creator-hub.operator-metrics.tsx` (Creator Hub Chrome). Panels: per-scene/world occupancy (presence), deploy funnel conversion, ban/admin activity. Emits `operator_dashboard_viewed` + `operator_visits_viewed` (with `peers`, `scenes`, `worlds` props from the presence snapshot) + `operator_dashboard_funnel_clicked`. Data: catalyrst-presence `/current` (+ `/current/scenes`, `/current/worlds`); deploy/ban funnels from emitted events. Fixture fallback when presence is empty/unreachable.
## 4. ADMIN persona - moderation / approvals
The DAO/curation admin: curation queue (collections), What's-On event moderation, community moderation. Decisions are the unit of work.

**F-ADM-1 Curation review funnel** - queue -> open item -> decide (approve/reject).

| Step           | Event                        | Emitting route(s) / data                                   |
| -------------- | ---------------------------- | ---------------------------------------------------------- |
| queue viewed   | `admin_queue_viewed`         | `packages/routes/app/routes/builder.curation.tsx` ({ pending count })       |
| item opened    | `admin_review_opened`        | `builder.curation.tsx` / collection detail                  |
| decided        | `admin_decision_submitted`   | `builder.curation.tsx` ({ decision: approve\|reject }, PATCH SIMULATED) |

- Data source: builder-server curation (`packages/data/src/lib/catalyst/builder-curation.ts`; `CollectionCurationAttributes`, status in pending\|approved\|rejected).
- Primary metric: approval throughput = `admin_decision_submitted` per session.
- Guardrails: `admin_queue_viewed` (queue depth does not grow unattended), reject rate stays sane.

**F-ADM-2 Event moderation funnel** (What's-On) - queue -> review event -> decision (approve/reject/feature).

| Step          | Event                       | Emitting route(s) / data                                    |
| ------------- | --------------------------- | ----------------------------------------------------------- |
| queue viewed  | `admin_events_queue_viewed` | `packages/routes/app/routes/landings.whatson-admin.tsx`                      |
| decided       | `admin_event_moderated`     | `landings.whatson-admin.tsx` ({ action }, PATCH `/events/api/events/{id}` SIMULATED, admin bearer fail-closed) |

- Primary metric: `admin_event_moderated` per session.
- Guardrails: `admin_events_queue_viewed`, feature/reject ratio.
### ADMIN dashboard
New route `packages/routes/app/routes/creator-hub.admin-metrics.tsx` (Admin chrome). Panels: curation queue depth + approval rate, events queue + moderation rate. Emits `admin_dashboard_viewed` + `admin_queue_viewed` ({ source, pending }) + `admin_dashboard_funnel_clicked`. Data: builder-server curation (`builder-curation.ts`) + events list (`whatson-admin.ts` / `events.ts`). Fixture fallback.
## 5. CAST persona - streamer go-live + watcher
Already partly instrumented by `landings.cast-stream.tsx` (`st_cast_console`, metric `cast_go_live_rate`). Canonical events stay `cast_*` (the persona prefix already matches).

**F-CAST-1 Go-live funnel (streamer)** - open console -> token checked -> permissions granted -> live.

| Step              | Event                        | Emitting route(s)                          |
| ----------------- | ---------------------------- | ------------------------------------------ |
| console opened    | `cast_console_opened`        | `packages/routes/app/routes/landings.cast-stream.tsx`      |
| token checked     | `cast_token_checked`         | same (existing guardrail)                   |
| permissions denied| `cast_permissions_denied`    | same (guardrail)                            |
| invalid token     | `cast_invalid_token`         | same (guardrail)                            |
| live              | `cast_go_live` / `cast_go_live_rate` | same (room join SIMULATED)          |

- Primary metric: `cast_go_live_rate` (existing).
- Guardrails: `cast_token_checked`, `cast_permissions_denied`, `cast_invalid_token`.

**F-CAST-2 Watcher funnel** (new surface) - watch link -> joined -> watching.

| Step          | Event                  | Emitting route(s)                              |
| ------------- | ---------------------- | ---------------------------------------------- |
| watch opened  | `cast_watch_opened`    | new `landings.cast-watch.tsx` (or cast-stream watcher mode) |
| watch joined  | `cast_watch_joined`    | same                                            |

- Primary metric: watch-join rate; Guardrail: stream-available rate.

CAST keeps its instrumentation inside its own story routes (already an XState console); no separate dashboard - its metrics roll up alongside CREATOR on the creator dashboard if desired.
## 6. Builder -> Creator Hub integration plan
The Builder lives as standalone `bd_*` screens (`builder.collections.tsx`, `builder.scenes.tsx`, `builder.curation.tsx`, `builder.worlds.tsx`, etc.) while the Creator Hub (`ch_*`) owns scene create/deploy/manage - two front doors to the same wallet's content. Unify under one chrome and one analytics rollup:

1. Single chrome: wrap the Builder screens in `CreatorHubChrome` (the same one `creator-hub.manage.tsx` uses) so Collections / Items / Scenes / Worlds / Curation become Creator Hub tabs. ui3 stays untouched - compose on the consumer side.
2. Route namespace: keep filesystem routes unique; treat `builder.*` as Creator Hub sub-areas; new dashboards land under `creator-hub.*-metrics.tsx`. No renames required.
3. Analytics rollup: existing `bd_*`/`ch_*` screen events stay for drill-down; ADD the persona rollup events (`creator_*`, `operator_*`, `admin_*`) at the same call sites. Map: `bd_collections_*`, `bd_item_*`, publish -> `creator_*` (F-CRE-1/2); `bd_scenes_*`, `bd_worlds_*`, `ch_deploy_*` -> `operator_*` (F-OP-1); `bd_curation_*` -> `admin_*` (F-ADM-1).
4. Shared session id: all surfaces already use `ensureSid` - the same `sid` cookie makes Builder -> Creator Hub -> Marketplace one funnel, not three anonymous users.
5. Data layer reuse: builder data libs (`builder-collections.ts`, `builder-curation.ts`, `builder-scenes.ts`) already encode the builder-server contracts; dashboards consume them directly (no new fetch layer).

Rollout: (a) land the three metrics dashboards behind the new Creator Hub tabs; (b) add the persona rollup events at existing `bd_*`/`ch_*` call sites; (c) fold Builder screens under the unified chrome.
## 7. Metabase / dashboard conventions
- One Metabase funnel question per F-* above, ordered by the step events; the primary metric is the last-step conversion; guardrails are saved as trend questions on the named guardrail events.
- Experiment surfaces emit `experiment_exposed` once (via `trackExposure`); funnel events carry `exp_key` + `variant` so analysis can segment by arm.
- Dashboards are loader-rendered (SSR, no client query lib): the route loader computes the summary from the catalyst/presence/builder data source; the dashboard's own `*_dashboard_viewed` event fires on mount.
