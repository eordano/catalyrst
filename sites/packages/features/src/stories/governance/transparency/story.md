---
id: governance-transparency
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A single-page DAO Transparency view surfacing treasury balance, monthly
    income/expenses, and committee members increases the share of governance
    visitors who understand where DAO funds go and stay engaged with governance.
  because: >-
    Users who land on the governance hub and want to understand the DAO's
    financial health have no obvious entry point today. A dedicated Transparency
    page with real token balances, monthly breakdowns, and the committee roster
    turns a passive visit into an informed one, reducing bounce and increasing
    downstream proposal engagement.
metric:
  primary: gv_transparency_viewed
  guardrails:
    - gv_transparency_committee_expanded
    - gv_transparency_dashboard_clicked
experiment:
  key: gv_transparency_page
  unit: session
  variants:
    - id: with-transparency
      weight: 1
      flags:
        showTransparency: true
  baseline: 0.10
  mde: 0.02
  min_sample: 6000
decision:
  rule: >-
    Ship if gv_transparency_viewed sessions are non-zero and gv_transparency_dashboard_clicked
    reaches at least 2% of viewers; hold if committee_expanded rate is zero (data
    not rendering) or bounce back to the home spikes.
---

# Governance Transparency -- DAO treasury, income/expenses, and committee roster

The DAO Transparency page (`/governance/transparency`) is the financial front door
of the governance hub. It surfaces four data layers in a single SSR-rendered,
no-JS-required view:

1. **Vesting contract** -- the DAO's 10-year, 222 MM MANA vesting contract:
   releasable amount and unvested balance.
2. **Current balance** -- aggregated token balances (MANA / USDC / USDT / DAI /
   ETH / MATIC / WETH).
3. **Monthly income/expenses** -- last-30-day totals with % delta vs the prior
   period and a collapsible detail breakdown (vesting releases, marketplace fees,
   grant disbursements, etc.).
4. **Committee roster** -- Security Advisory Board, DAO Council, and Wearable
   Curation Committee with member name + hue avatar tiles.

Governance is NOT a Catalyst service (the DAO runs on Snapshot + Aragon
externally; `governance.decentraland.org/api/transparency` returns 404). All
data is fixture/deferred, derived from `GvTransparency.jsx` shapes + DAO docs.

Route: `/governance/transparency`. No URL parameters; the page is a single SSR
surface with no wizard steps.

- **Primary metric:** `gv_transparency_viewed` (page-load impressions per session).
- **Guardrails / downstream:** `gv_transparency_committee_expanded` (user expanded
  the view-more in a monthly total card) and `gv_transparency_dashboard_clicked`
  (outbound link to the full transparency dashboard).
- **Events:**
  - `gv_transparency_viewed` -- on mount (view impressions).
  - `gv_transparency_dashboard_clicked` -- click on the "Transparency Dashboard"
    sidebar link.
  - `gv_transparency_committee_expanded` -- click on the "View N more..." toggle
    inside a monthly total card (property: `{card: "income"|"expenses"}`).
