---
id: governance-submit-proposal
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A grouped, URL-addressable category picker (Common Actions / Bidding /
    Governance Process) that deep-links straight to each submit wizard helps a
    would-be author pick the right proposal type and start a submission faster.
  because: >-
    The DAO offers a dozen proposal categories with overlapping names; a clear,
    grouped hub -- with an explicit Add/Remove chooser for Catalyst/POI/Hiring --
    reduces the chance an author picks the wrong type or bounces, raising the
    share of hub views that proceed into a submit flow.
metric:
  primary: gv_submit_category_selected
  guardrails:
    - gv_submit_hub_viewed
    - gv_submit_group_filtered
    - gv_submit_chooser_opened
decision:
  rule: >-
    Ship if the category-selection rate
    (gv_submit_category_selected / gv_submit_hub_viewed) improves by at least the
    MDE with no regression in hub-view volume; otherwise hold and revisit the
    grouping/copy.
experiment:
  key: gv_submit_category_picker
  unit: session
  variants:
    - id: grouped-picker
      weight: 1
      flags:
        groupedHub: true
        addRemoveChooser: true
  baseline: 0.35
  mde: 0.04
  min_sample: 4000
---

# Governance -- Submit a proposal (category picker hub)

The Submit hub (`/governance/submit`) is a **loader + components** surface (no
machine -- this is the picker that *launches* the per-category submit wizards). The
loader reads `?group` (common | bidding | process) and `?request` (add | remove)
from the URL, parses the local fixture
(`app/fixtures/governance-submit-proposal.json`) with a tolerant zod schema, and
returns plain data so the page renders fully with JS disabled (data is in the
HTML). Analytics fire on the client.

Governance is **NOT** a Catalyst service (the DAO runs on Snapshot + Aragon
externally), and the Submit hub upstream is a purely static category list, so the
data is **SIMULATED / derived** from `decentraland/governance-ui`
(`src/pages/submit/index.tsx` groups + order, `src/types/proposals.ts` enums,
`src/utils/locations.ts` route slugs) and clearly marked fixture/deferred. The
final proposal submission itself lives in the per-category wizard routes (e.g.
`/governance/submit/poll`) and the on-chain/Snapshot publish is stubbed there.

## Journey (URL-addressable)

- `/governance/submit` -- the category picker (Common Actions + Bidding & Tendering
  + Governance Process groups). Fires `gv_submit_hub_viewed`.
- `/governance/submit?group=common` (also `bidding` / `process`) -- filter/anchor
  to one group. Fires `gv_submit_group_filtered`.
- click a category banner (e.g. **Poll**) -> deep-links to the matching wizard
  route `/governance/submit/poll`. Fires `gv_submit_category_selected`.
- **Catalyst / POI / Hiring** rows open the Add/Remove chooser instead of
  navigating: `?request=add` / `?request=remove` is set first (URL-addressable)
  and only then does the chosen option link onward. Fires
  `gv_submit_chooser_opened` (on open) and `gv_submit_category_selected` (on the
  add/remove choice).

## Metrics

- **Primary:** `gv_submit_category_selected` -- a category (or add/remove choice)
  was picked and the author was routed into a submit wizard.
- **Guardrails:** `gv_submit_hub_viewed` (volume), `gv_submit_group_filtered`
  (group filter usage), `gv_submit_chooser_opened` (chooser engagement).

Single shipping variant (`grouped-picker`); the schema is kept valid for the
readout tooling and deterministic bucketing.
