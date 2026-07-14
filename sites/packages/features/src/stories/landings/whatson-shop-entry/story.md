---
id: whatson-shop-entry
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Sessions browsing the events schedule are planning to attend something, and
    attending is a dress-up occasion in Decentraland -- a "dress for the event"
    cross-promo between the schedule and the event links converts
    event-browsing sessions into Shop visits.
  because: >-
    /whats-on is intent-rich traffic (people picking an event to show up at)
    that today offers no path into the Shop at all. Dressing up for an event is
    an existing behavior the surface never taps -- the pill tests whether the
    "dress for the event" label alone earns the click, the rail tests whether
    showing fresh on-sale emotes earns it better.
metric:
  primary: lp_whatson_shop_open_rate
  numerator: lp_whatson_shop_opened
  guardrails:
    - lp_whatson_viewed
    - lp_event_card_clicked
    - lp_whatson_shop_entry_shown
experiment:
  key: lp_whatson_shop_entry
  unit: session
  variants:
    - id: base
      weight: 90
      flags:
        shopEntry: base
    - id: pill
      weight: 5
      flags:
        shopEntry: pill
    - id: rail
      weight: 5
      flags:
        shopEntry: rail
  baseline: 0.0
  mde: 0.02
decision:
  rule: >-
    Draft -- do not ship from this readout. base is control (today's /whats-on,
    no Shop entry; lp_whatson_shop_opened is structurally ~0 there). Promote
    pill or rail only if lp_whatson_shop_open_rate (lp_whatson_shop_opened /
    experiment_exposed) beats base with 95% confidence AND the schedule
    guardrail holds: lp_event_card_clicked per lp_whatson_viewed must not
    regress vs base -- the entry must add Shop visits, not cannibalize event
    attendance. lp_whatson_shop_entry_shown must track experiment_exposed on
    pill/rail (a gap means the entry failed to render). Otherwise keep base.
---

# What's On -- a "dress for the event" Shop entry on the schedule

A second experiment layered on the `/whats-on` surface (its own key,
`lp_whatson_shop_entry`, assigned independently of the `lp_whatson_feed` host
story by the sid hash). The story dir lives in `landings/` because the surface
is the public landings-family page (`LdWhatsOnPage`, `lp_whatson_*` events);
`admin/whats-on` is the legacy host story id and stays as-is. Three arms,
assigned per session:

- **base** -- today's `/whats-on` page, unchanged. No Shop entry renders.
- **pill** -- a "Dress for tonight -- Shop emotes & wearables" pill between
  the schedule and the all-events links, styled like the existing
  `whatson-route__pill` chips -> `/shop?from=whatson-shop-entry`.
- **rail** -- a "Gear up before you go / Fresh emotes on sale" rail in the
  same slot, showing up to four live on-sale emotes (real `fetchCatalog`
  reading, recently listed first -- the Shop's default sort) plus an
  "Open the Shop" CTA. Item cards -> `/marketplace/:id?from=whatson-shop-entry`;
  the CTA -> `/shop?from=whatson-shop-entry`. If the catalog reading is
  unavailable the rail says so and keeps the CTA -- no invented items, never a
  dead-end.

Journey:

1. `/whats-on` -- the loader resolves the arm (deterministic sid bucket;
   `?arm=` / `?variant=lp_whatson_shop_entry:<arm>` force an arm for preview).
   Gated by `WHATSON_SHOP_ENTRY_EXPERIMENT` (or a runtime flags-service
   override row): unset, every session gets base and no exposure fires;
   previews never count as exposure either way.
2. `experiment_exposed` fires in the loader for assigned sessions.
   `lp_whatson_shop_entry_shown {variant}` fires when a pill/rail entry paints
   (so it tracks experiment_exposed per treatment arm).
3. Any click-through into the Shop fires the shared `lp_whatson_shop_opened
   {variant, target, item_id}` -- the primary-metric numerator -- with `target`
   one of `pill`, `rail_cta`, `rail_item` (item_id set only for `rail_item`).
4. The existing schedule events (`lp_whatson_viewed`, `lp_event_card_clicked`)
   keep firing under the host story and serve as the
   don't-cannibalize-event-attendance guardrail.

Primary metric `lp_whatson_shop_open_rate` = `lp_whatson_shop_opened` /
`experiment_exposed`, per arm -- derivable with
`npm run story:readout -- landings/whatson-shop-entry` and the
catalyrst-telemetry experiment readout.

Preview (each arm forceable regardless of bucket):

- base: `/whats-on?arm=base`
- pill: `/whats-on?arm=pill`
- rail: `/whats-on?arm=rail`
