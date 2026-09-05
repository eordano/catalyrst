---
id: governance-project-detail
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A project detail page that leads with a persistent funding sidebar (vesting
    progress + next vesting step) alongside the tabbed General / Milestones /
    Updates / Activity rail keeps grant supporters engaged and drives them into
    the project's updates and vesting contracts.
  because: >-
    Grant/bid backers want to know where the money is going; surfacing vested vs
    released amounts and the next vesting step up-front (rather than buried in an
    Activity log) answers the "is this on track?" question immediately, so a
    larger share of project views explore milestones/updates instead of bouncing.
metric:
  primary: gv_project_viewed
  guardrails:
    - gv_project_tab_viewed
    - gv_project_vesting_clicked
experiment:
  key: gv_project_detail
  unit: session
  variants:
    - id: funding-sidebar
      weight: 1
      flags:
        showFundingSidebar: true
        showVestingProgress: true
  baseline: 0.4
  mde: 0.04
  min_sample: 4000
decision:
  rule: >-
    Ship if gv_project_viewed sessions go on to gv_project_tab_viewed (any tab
    beyond General) at or above the MDE over baseline with no regression in
    gv_project_vesting_clicked; otherwise hold and revisit the sidebar layout.
---

# Governance project detail -- grant/bid with funding & vesting

The Project Detail page (`/governance/projects/:id`) is the single grant/bid view
of the DAO governance hub (decentraland.org/governance `src/pages/project.tsx` ->
`ProjectView`). It is a simple **loader + components** surface (priority `spec`,
no multi-step XState wizard): the loader mints a session id, resolves the project,
emits the view + exposure events, and renders ui3's `GvProjectDetail` inside
`GovernanceChrome`. There is nothing to submit here -- it is read-only.

Governance is NOT a Catalyst service (the DAO runs on Snapshot + Aragon), so the
project is read **live** from the upstream governance API
(`https://governance.decentraland.org/api/projects/:id`, public, no auth) with a
faithful fallback to the bundled fixture `app/fixtures/governance-project-detail.json`
(derived from the upstream `Project` / `Vesting` model shapes). The funding
sidebar's vesting amounts (vested / released / total) and percentages are real;
nothing is written.

Each journey step is URL-addressable via `?tab=`:

- `/governance/projects/:id` -- ProjectHero + the vertical tab rail (defaults to
  General Info).
- `?tab=general` -- status card, About (markdown), Project Links, Personnel.
- `?tab=milestones` -- the milestones list.
- `?tab=updates` -- the project updates list (each links out to its update detail).
- `?tab=activity` -- the activity log; the funding sidebar (VestingProgress) is
  persistent across all tabs.
- An unknown `:id` renders the NotFound state.

Flow / events:

- `gv_project_viewed {project_id, status, source}` -- on load (once per mount).
- `gv_project_tab_viewed {project_id, tab}` -- when a tab beyond General becomes
  active (deep-linked `?tab=` does NOT double-fire the initial view).
- `gv_project_vesting_clicked {project_id, vesting_id}` -- a vesting-contract link
  in the funding sidebar is opened.

- **Primary metric:** `gv_project_viewed` (project-detail views per session).
- **Guardrails / downstream:** `gv_project_tab_viewed` (tab exploration),
  `gv_project_vesting_clicked` (vesting-contract click-through).

Single shipping variant (`funding-sidebar`); the schema stays fully valid so the
readout tooling and deterministic bucketing work unchanged if a control arm
(funding sidebar hidden) is added later.
