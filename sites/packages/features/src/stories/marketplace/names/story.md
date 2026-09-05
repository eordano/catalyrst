---
id: marketplace-names
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A live, paginated NAMEs grid (registered Decentraland NAMEs with an "on sale
    only" toggle) lets visitors discover claimed names that are being resold, raising
    the share of NAMEs sessions that either open a listed name or start a claim
    versus a static placeholder grid.
  because: >-
    Visitors arrive wanting a memorable NAME; surfacing real registered names with
    their listing price up front, an on-sale filter, and a clear "claim a fresh
    NAME" path reduces the effort to either buy a resold name or mint a new one, and
    URL-addressable filters/pages make every result shareable.
metric:
  primary: mk_names_viewed -> mk_names_card_clicked / mk_names_claim_started conversion
  numerator: mk_names_claim_clicked
  denominator: mk_names_checked
  guardrails:
    - mk_names_buy_clicked
    - mk_names_signin_gated
experiment:
  key: marketplace_names
  unit: session
  variants:
    - id: default
      weight: 100
      flags: {}
  baseline: 0.12
  mde: 0.03
decision:
  rule: >-
    Ship the live paginated NAMEs grid if the names-to-action conversion
    (sessions with mk_names_card_clicked OR mk_names_claim_started / sessions with
    mk_names_viewed) beats the static-grid baseline with 95% confidence; otherwise
    iterate on the on-sale filter and the claim CTA.
---

# Marketplace -- NAMEs marketplace (browse registered names grid)

Visitor opens the NAMEs grid (live `/credits/v1/nfts?category=ens`, with the open
listing order embedded per row) and narrows it with URL-addressable controls: an
"on sale only" toggle and page navigation.

Journey + metrics:

- `/marketplace/names` loads the SSR grid of registered NAMEs ->
  `mk_names_viewed { count, total, on_sale, page }`.
- Toggling on-sale via the URL (`?onSale=true`) re-derives the listed subset ->
  `mk_names_filter_applied { filter: "onSale", value }`.
- Paging via the URL (`?page=2` / `?skip=`) refetches the next slice on the server
  -> `mk_names_paginated { page, skip }`.
- Clicking an EnsCard -> `mk_names_card_clicked { nft_id, on_sale }` -> navigates to
  the asset detail `/marketplace/:id`.
- The "Claim NAME" tab (`/marketplace/names?step=claim`, `MkClaimNamePage`) lets a
  visitor mint a brand-new NAME; opening it emits
  `mk_names_claim_started { source: "names_grid" }`. The on-chain mint itself is
  SIMULATED -- the ui3 claim page drives the input/availability/confirm states, but
  no transaction is broadcast (final commit is a clearly-noted stub).

Data is live with a fixture fallback (`app/fixtures/marketplace-names.json`,
`_source` = the live ENS nfts endpoint) so the grid renders even when Catalyst is
unreachable. NAMEs are Ethereum-only ERC-721s, so cards always show the Ethereum
network line; only names with an open order show a MANA price.
