---
id: creator-wearable-publish-collection
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A staged wearables-publish wizard that shows the collection summary, an
    itemised MANA publish-fee breakdown, and explicit content/curation terms
    BEFORE asking the creator to pay the MANA fee increases the share of started
    publishes that reach a real submitted-for-curation result.
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

# Publish a collection

The production route `/create/wearables/publish?collection=<id>` uses signed Builder drafts, a Polygon wallet and Foundation Builder. Foundation requests use a fixed-target same-origin relay because its CORS policy does not allow this site; signatures remain bound to the upstream method/path. Storybook retains the simulated machine for isolated design previews.

## Creator needs and assumptions

- Review the collection and exact publication fee before approving a transaction.
- Understand that collection deployment and Foundation review submission are separate steps.
- Recover interrupted uploads, wallet responses and indexing without paying twice.
- A connected identity owns the draft; a transaction-capable wallet must select the same account. Builder has its Polygon RPC configured. Foundation Builder, its indexer and forum are available.
- This flow covers standard wearable/emote collections. Linked-provider collections follow their provider registration and quota workflow.

## Runtime flow

1. **Summary:** fetch the selected wallet's collection. Missing collection, sign-in, loading and retry states have no simulated publish action. URL parameters cannot manufacture payment or success.
2. **Review:** inspect actual uploaded models, persist model/animation metrics, generate missing wearable thumbnails, validate Foundation metadata and get exact rarity fees from the current Polygon collection contract. Emotes require their supplied thumbnail. A pending or paid publication instead offers continuation.
3. **Cost:** show rarity subtotals and the exact MANA total. Compact table values are explicitly approximate; the full amount remains visible. Gas is separate.
4. **Terms:** require consent and a valid email shared with Foundation for the submission.
5. **Payment:** freeze the reviewed revision, synchronize files and metadata with Foundation, verify its predicted contract address and item order, record terms, approve exactly the reviewed amount, recheck fees and send the collection transaction. Rust verifies the finalized receipt and deployed contract before assigning item IDs.
6. **Submission:** wait for Foundation indexing, verify blockchain item IDs and create/reuse its review topic. Only then display curation submission success with real transaction and forum links. Entity deployment belongs to subsequent curator approval.

## Recovery and constraints

- Upload failures occur before payment; retry preserves item IDs and hashes.
- Prepared drafts remain frozen until resumed or explicitly unlocked for editing.
- Pending hashes and the server's signing claim survive reload; unresolved wallet sends require transaction-hash recovery. Another tab cannot send the same publication again.
- A paid collection skips another approval/payment and resumes only the Foundation handoff. Lost forum responses recover the stored topic link.
- Chain/account/contract mismatches and changed fees stop the flow; they never become zero-cost quotes or simulated successes.
- Leaving the screen aborts polling and network work. Account changes remount the flow. Emails are not included in telemetry.

## Measurement and evidence

`bd_publish_collection_started`, `bd_publish_collection_cost_shown` and `bd_publish_collection_terms_accepted` describe review progression. Real `bd_publish_fee_paid` carries `simulated: false`; real `bd_publish_submitted` carries `stub: false`. Unknown historical fees on resumed submissions are omitted. Storybook events remain simulated.

Success requires independently confirmed deployment plus a valid Foundation review topic. The primary metric remains submissions / starts; payment and error behavior must be audited alongside conversion.

Data/wallet tests cover exact fees, preparation before payment, concurrent signing, receipt recovery, item order, delayed indexing and forum retry. `tools/screen-tour/verify-collection-publication.mts` drives the real production route with local Rust/PostgreSQL drafts and controlled Polygon/Foundation responses, including thumbnail generation, consent, upload failure, indexing delay, reload and lost forum response. It does not spend funds or create a real forum topic.
