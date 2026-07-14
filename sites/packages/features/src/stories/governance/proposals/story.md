---
id: governance-proposals
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A filterable proposals list (category / status / search as URL params) helps
    visitors find and open a relevant proposal faster.
  because: >-
    The DAO has thousands of proposals across many categories; URL-addressable
    filters let a visitor narrow to what they care about, raising the share of
    list views that end in opening a proposal detail.
metric:
  primary: gv_proposal_viewed
  guardrails:
    - gv_proposals_viewed
    - gv_proposals_filtered
experiment:
  key: gv_proposals_browse
  unit: session
  variants:
    - id: filterable-list
      weight: 1
      flags:
        urlFilters: true
  baseline: 0.3
  mde: 0.03
  min_sample: 5000
decision:
  rule: >-
    Ship if the detail-open rate (gv_proposal_viewed / gv_proposals_viewed)
    improves by at least the MDE with no guardrail regression; otherwise hold.
---

# Governance proposals -- browse and open a proposal

The proposals list (`/governance/proposals`) is a loader + component surface
(no machine). The loader reads `?category` / `?status` / `?search` from the URL,
loads the local fixture (`app/fixtures/governance.json`), best-effort enriches
author labels via `/lambdas/profiles` (never blocking SSR), dehydrates React
Query and emits the list-view event. The component composes ui3's
`GovernanceProposals`; a graceful `GvNotFound` covers unknown detail ids.

Governance is NOT a Catalyst service (Snapshot + Aragon externally), so the list
and detail are fixture-driven and marked fixture/deferred; vote submission is
stubbed.

- **Primary metric:** `gv_proposal_viewed` -- detail-open rate from the list,
  derivable from `gv_proposals_viewed` + `gv_proposal_clicked` + `gv_proposal_viewed`.
- **Guardrails:** `gv_proposals_viewed` volume and `gv_proposals_filtered` usage.
- **Events:** `gv_proposals_viewed` on load (with count), `gv_proposals_filtered`
  on a filter change, `gv_proposal_clicked` on a card click, `gv_proposal_viewed`
  on detail open.

Single shipping variant (`filterable-list`); schema kept valid for the readout
tooling and deterministic bucketing.
