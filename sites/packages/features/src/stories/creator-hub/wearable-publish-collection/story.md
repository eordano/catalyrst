---
id: creator-wearable-publish-collection
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A staged wearables-publish wizard that shows the collection summary, an
    itemised MANA publish-fee breakdown, and explicit content/curation terms
    BEFORE asking the creator to pay the MANA fee increases the share of started
    publishes that reach the submitted-for-curation step, even with the on-chain
    payment simulated.
  because: >-
    Surfacing the exact per-item MANA cost and the curation terms up front
    removes the two biggest sources of abandonment at the pay step (sticker
    shock and uncertainty about what curation entails), so more creators who
    open the publish flow follow through to paying the fee and submitting
    instead of bailing at an opaque single-shot "pay & publish" button.
metric:
  primary: bd_publish_submit_rate
  numerator: bd_publish_submitted
  denominator: bd_publish_collection_started
  guardrails:
    - bd_publish_collection_started
    - bd_publish_collection_cost_shown
    - bd_publish_fee_paid
experiment:
  key: bd_wearable_publish_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if bd_publish_submit_rate improves by at least the MDE with no
    guardrail regression (publish-flow opens, cost-shown volume, and fee-paid
    volume hold, and the empty/no-items collection stays blocked without
    crashing); otherwise hold.
---

# Publish a wearables collection: pay the MANA fee and submit for curation

The publish wizard (`/create/wearables/publish`) breaks publishing a
wearable/emote collection into explicit, URL-addressable steps:

1. **Summary** (`?step=summary`) -- show the collection and the items that will be
   published (ui3 `BdCollectionDetail`). Emits `bd_publish_collection_started`
   `{ id, itemCount }`.
2. **Cost** (`?step=cost`) -- the MANA publish-fee breakdown, a flat per-item fee
   grouped by rarity tier and rolled up to a total. Emits
   `bd_publish_collection_cost_shown` `{ mana }`.
3. **Terms** (`?step=terms`) -- accept the content + curation terms. Emits
   `bd_publish_collection_terms_accepted`.
4. **Pay** (`?step=pay`) -- approve MANA and sign the publish. Emits
   `bd_publish_fee_paid` `{ mana, tx_hash }`. The on-chain MANA payment is
   **SIMULATED**.
5. **Submitted** (`?step=submitted`) -- the collection is submitted for curation
   review. Emits `bd_publish_submitted` `{ id, itemCount, mana }`. The curation
   submission is a **stub**.

- **Primary metric:** `bd_publish_submit_rate` =
  `bd_publish_submitted` / `bd_publish_collection_started`.
- **Guardrails:** publish-flow open volume (`bd_publish_collection_started`),
  cost-shown volume (`bd_publish_collection_cost_shown`), and fee-paid volume
  (`bd_publish_fee_paid`) must stay healthy.
- **Events:** `experiment_exposed` (on render), `bd_publish_collection_started`
  `{ id, itemCount }`, `bd_publish_collection_cost_shown` `{ mana }`,
  `bd_publish_collection_terms_accepted`, `bd_publish_fee_paid` `{ mana, tx_hash }`
  (sim), `bd_publish_submitted` `{ id, itemCount, mana }` (stub).

Data reality: the per-item MANA publish fee mirrors the on-chain Rarities
contract item price (a flat per-item fee, 100 MANA/item -- it does NOT vary by
rarity tier). The collection + items are read LIVE from the builder
(`GET /v1/collections/{id}/items` at SSR, re-read through the signed-fetch
session once a wallet connects) -- there is **no fixture fallback**: when
nothing can be read the route shows an explicit empty-state notice and the
wizard routes to **blocked** (no fixture exists; the old capture was deleted
in the dead-code sweep). The on-chain MANA payment and the
curation submission are simulated/stubbed and the UI says so on the pay /
submitted panels; the flow, states, fee math, and telemetry are real. An empty
/ no-items collection routes to the graceful **blocked** state and never
crashes.
