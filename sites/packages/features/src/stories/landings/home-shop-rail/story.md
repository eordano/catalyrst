---
id: home-shop-rail
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Giving the decentraland.org home landing -- the highest-traffic front door
    -- an explicit Shop entry point (a slim banner CTA or a rail of live
    on-sale drops) between the hero and the feature rails turns
    evaluate-and-leave sessions into Shop visits.
  because: >-
    The home landing is where first-time visitors decide whether Decentraland
    is worth their time, and today it offers zero path into the Shop; sessions
    that are not ready to download simply leave. A visible entry converts some
    of that existing traffic into Shop visits -- the cta arm tests the cheapest
    possible affordance, the rail arm tests whether showing live drops earns
    the click better than a label.
metric:
  primary: lp_home_shop_open_rate
  numerator: lp_home_shop_opened
  guardrails:
    - lp_home_viewed
    - lp_home_rail_viewed
    - lp_home_rail_clicked
    - lp_home_download_clicked
    - lp_home_shop_entry_shown
experiment:
  key: lp_home_shop_rail
  unit: session
  variants:
    - id: base
      weight: 90
      flags:
        shopEntry: base
    - id: cta
      weight: 5
      flags:
        shopEntry: cta
    - id: rail
      weight: 5
      flags:
        shopEntry: rail
  baseline: 0.0
  mde: 0.02
decision:
  rule: >-
    Draft -- do not ship from this readout. base is control (today's home, no
    Shop entry; lp_home_shop_opened is structurally ~0 there). Promote cta or
    rail only if lp_home_shop_open_rate (lp_home_shop_opened /
    experiment_exposed) beats base with 95% confidence AND the hero guardrail
    holds: lp_home_download_clicked per lp_home_viewed must not regress vs
    base -- download CTR is the running lp_home_hero_cta experiment's primary
    metric, so the shop entry must add Shop visits, not steal download clicks.
    lp_home_shop_entry_shown must track experiment_exposed on cta/rail (a gap
    means the entry failed to render). Otherwise keep base.
---

# Home landing -- a Shop entry between the hero and the feature rails

A second experiment layered on the `/landings/home` surface (its own key,
`lp_home_shop_rail`, assigned independently of the running `lp_home_hero_cta`
hero experiment by the sid hash). Three arms, assigned per session:

- **base** -- today's home landing, unchanged. No Shop entry renders; the hero
  markup and both hero-experiment arms stay pixel-identical.
- **cta** -- a slim full-width "Shop the latest wearable drops" banner CTA
  between the hero download CTA and the feature rails ->
  `/shop?from=home-shop-rail`.
- **rail** -- a "From the Shop / Fresh drops on sale" rail in the same slot,
  showing up to six live on-sale items (real `fetchCatalog` reading, recently
  listed first) plus an "Open the Shop" CTA. Item cards ->
  `/marketplace/:id?from=home-shop-rail`; the CTA ->
  `/shop?from=home-shop-rail`. If the catalog reading is unavailable the rail
  says so and keeps the CTA -- no invented items, never a dead-end.

Journey:

1. `/landings/home` -- the loader resolves the arm (deterministic sid bucket;
   `?arm=` / `?variant=lp_home_shop_rail:<arm>` force an arm for preview).
   Draft until activated: a runtime flags-service row (or the
   `HOME_SHOP_RAIL_EXPERIMENT` env var) turns it on; otherwise every session
   gets base and no exposure fires. Previews never count as exposure either
   way.
2. `experiment_exposed` fires in the loader for assigned sessions.
   `lp_home_shop_entry_shown {variant}` fires when a cta/rail entry paints (so
   it tracks experiment_exposed per treatment arm).
3. Any click-through into the Shop fires the shared `lp_home_shop_opened
   {variant, target, item_id}` -- the primary-metric numerator -- with `target`
   one of `cta`, `rail_cta`, `rail_item` (item_id set only for `rail_item`).
4. The existing home events (`lp_home_viewed`, `lp_home_rail_viewed`,
   `lp_home_rail_clicked`, `lp_home_download_clicked`) keep firing under the
   `landings/home` story and serve as the don't-cannibalize guardrails --
   download CTR above all, since it is the hero experiment's primary metric.

Primary metric `lp_home_shop_open_rate` = `lp_home_shop_opened` /
`experiment_exposed`, per arm -- derivable with
`npm run story:readout -- landings/home-shop-rail` and the
catalyrst-telemetry experiment readout.

Preview (each arm forceable regardless of bucket):

- base: `/landings/home?arm=base`
- cta: `/landings/home?arm=cta`
- rail: `/landings/home?arm=rail`
