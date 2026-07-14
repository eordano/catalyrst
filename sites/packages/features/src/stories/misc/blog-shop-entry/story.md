---
id: blog-shop-entry
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Giving the blog index -- engaged, brand-warm content sessions -- an explicit
    Shop entry point (a drops cross-promo card or a rail of live fresh drops)
    raises the share of sessions that open the Shop.
  because: >-
    Blog readers are already invested enough to read marketing content that
    regularly announces wearable drops, yet /blog offers no path into the Shop
    at all today. A visible entry above the post grid turns content-reading
    sessions into Shop visits -- the card tests the plain affordance, the rail
    tests whether showing live fresh drops earns the click better than a label.
metric:
  primary: lp_blog_shop_open_rate
  numerator: lp_blog_shop_opened
  guardrails:
    - lp_blog_viewed
    - lp_blog_post_clicked
    - lp_blog_shop_entry_shown
experiment:
  key: lp_blog_shop_entry
  unit: session
  variants:
    - id: base
      weight: 90
      flags:
        shopEntry: base
    - id: card
      weight: 5
      flags:
        shopEntry: card
    - id: rail
      weight: 5
      flags:
        shopEntry: rail
  baseline: 0.0
  mde: 0.02
decision:
  rule: >-
    Draft -- do not ship from this readout. base is control (today's /blog, no
    Shop entry; lp_blog_shop_opened is structurally ~0 there). Promote card or
    rail only if lp_blog_shop_open_rate (lp_blog_shop_opened /
    experiment_exposed) beats base with 95% confidence AND the content guardrail
    holds: lp_blog_post_clicked per lp_blog_viewed must not regress vs base --
    the entry must add Shop visits, not eat post reads. lp_blog_shop_entry_shown
    must track experiment_exposed on card/rail (a gap means the entry failed to
    render). Otherwise keep base.
---

# Blog -- a Shop entry point above the post grid

A second experiment layered on the `/blog` surface (its own key,
`lp_blog_shop_entry`, assigned independently of `lp_blog_index` by the sid
hash). Three arms, assigned per session:

- **base** -- today's `/blog` index, unchanged. No Shop entry renders.
- **card** -- a single promo card strip above the post grid: "Wearable drops,
  straight from the creators" with a "Shop the latest" call to action ->
  `/shop?from=blog-shop-entry`.
- **rail** -- a "Fresh drops on sale" rail in the same slot, showing up to four
  live on-sale items (real `fetchCatalog` reading, most recently listed first;
  image, name, credits-over-MANA price) plus an "Open the Shop" CTA. Item
  cards -> `/marketplace/:id?from=blog-shop-entry`; the CTA ->
  `/shop?from=blog-shop-entry`. If the catalog reading is unavailable the rail
  says so and keeps the CTA -- no invented items, never a dead-end.

Journey:

1. `/blog` -- the loader resolves the arm (deterministic sid bucket;
   `?arm=` / `?variant=lp_blog_shop_entry:<arm>` force an arm for preview).
   Gated by `BLOG_SHOP_ENTRY_EXPERIMENT` (or a runtime flags-service override
   row): inactive, every session gets base and no exposure fires; previews
   never count as exposure either way.
2. `experiment_exposed` fires in the loader for assigned sessions.
   `lp_blog_shop_entry_shown {variant}` fires when a card/rail entry paints (so
   it tracks experiment_exposed per treatment arm).
3. Any click-through into the Shop fires the shared `lp_blog_shop_opened
   {variant, target, item_id}` -- the primary-metric numerator -- with `target`
   one of `card`, `rail_cta`, `rail_item` (item_id set only for `rail_item`).
4. The existing blog events (`lp_blog_viewed`, `lp_blog_post_clicked`) keep
   firing under their own story and serve as the don't-eat-post-reads
   guardrail; the post-click tracker only matches `/blog/:slug` hrefs, so shop
   clicks cannot double-count as `lp_blog_post_clicked`.

Primary metric `lp_blog_shop_open_rate` = `lp_blog_shop_opened` /
`experiment_exposed`, per arm -- derivable with
`npm run story:readout -- misc/blog-shop-entry` and the catalyrst-telemetry
experiment readout.

Preview (each arm forceable regardless of bucket):

- base: `/blog?arm=base`
- card: `/blog?arm=card`
- rail: `/blog?arm=rail`
