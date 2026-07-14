---
id: places-shop-entry
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Giving the Places explorer -- the front door of the explore family -- an
    explicit Shop entry point (a header pill or a rail of live on-sale items)
    raises the share of sessions that open the Shop.
  because: >-
    The Shop's checkout funnel converts, but too few sessions ever reach it to
    measure anything downstream; /places is where explore sessions already are,
    and today it offers no path into the Shop at all. A visible entry turns
    existing explore traffic into Shop visits -- the low-key pill tests the
    cheapest possible affordance, the rail tests whether showing live on-sale
    items earns the click better than a label.
metric:
  primary: pl_shop_open_rate
  numerator: pl_shop_opened
  guardrails:
    - place_list_viewed
    - place_card_clicked
    - pl_shop_entry_shown
experiment:
  key: places_shop_entry
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
    Draft -- do not ship from this readout. base is control (today's /places, no
    Shop entry; pl_shop_opened is structurally ~0 there). Promote pill or rail
    only if pl_shop_open_rate (pl_shop_opened / experiment_exposed) beats base
    with 95% confidence AND the explore guardrail holds: place_card_clicked per
    place_list_viewed must not regress vs base -- the entry must add Shop visits,
    not cannibalize jump-ins. pl_shop_entry_shown must track experiment_exposed
    on pill/rail (a gap means the entry failed to render). Otherwise keep base.
---

# Places -- a Shop entry point on the explore front door

A second experiment layered on the `/places` surface (its own key,
`places_shop_entry`, assigned independently of `browse-places-live-signals` by
the sid hash). Three arms, assigned per session:

- **base** -- today's `/places` page, unchanged. No Shop entry renders.
- **pill** -- a "Shop wearables & emotes" pill between the page header and the
  category pills, styled like the existing `pl__pill` chips ->
  `/shop?from=places-shop-entry`.
- **rail** -- a "From the Shop / On sale right now" banner in the same slot,
  showing up to four live on-sale items (real `fetchCatalog` reading, cheapest
  first -- the same loader the Shop overview's trending row uses) plus an
  "Open the Shop" CTA. Item cards -> `/marketplace/:id?from=places-shop-entry`;
  the CTA -> `/shop?from=places-shop-entry`. If the catalog reading is
  unavailable the rail says so and keeps the CTA -- no invented items, never a
  dead-end.

Journey:

1. `/places` -- the loader resolves the arm (deterministic sid bucket;
   `?arm=` / `?variant=places_shop_entry:<arm>` force an arm for preview).
   Gated by `PLACES_SHOP_ENTRY_EXPERIMENT`: unset, every session gets base and
   no exposure fires; previews never count as exposure either way.
2. `experiment_exposed` fires in the loader for assigned sessions.
   `pl_shop_entry_shown {variant}` fires when a pill/rail entry paints (so it
   tracks experiment_exposed per treatment arm).
3. Any click-through into the Shop fires the shared `pl_shop_opened {variant,
   target, item_id}` -- the primary-metric numerator -- with `target` one of
   `pill`, `rail_cta`, `rail_item` (item_id set only for `rail_item`).
4. The existing browse-places events (`place_list_viewed`,
   `place_card_clicked`) keep firing under their own story and serve as the
   don't-cannibalize-jump-ins guardrail.

Primary metric `pl_shop_open_rate` = `pl_shop_opened` / `experiment_exposed`,
per arm -- derivable with `npm run story:readout -- misc/places-shop-entry` and
the catalyrst-telemetry experiment readout.

Preview (each arm forceable regardless of bucket):

- base: `/places?arm=base`
- pill: `/places?arm=pill`
- rail: `/places?arm=rail`
