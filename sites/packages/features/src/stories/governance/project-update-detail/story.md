---
id: governance-project-update-detail
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A focused, single-update reading view that surfaces project health, the
    funds-released/disclosed cards, a per-category financial breakdown, and the
    Discourse thread keeps DAO members on the update long enough to read it and
    nudges them toward the discussion.
  because: >-
    Grant accountability lives in the progress updates. When members land on a
    bare list they skim and bounce; a dedicated update page with the health
    banner up top, the financial cards, and the comment thread inline gives a
    concrete reason to read the whole report and engage with the discussion.
metric:
  primary: gv_update_viewed
  guardrails:
    - gv_update_comments_viewed
    - gv_update_notfound
experiment:
  key: gv_update_detail
  unit: session
  variants:
    - id: focused-read
      weight: 1
      flags:
        showFinancials: true
        showComments: true
  baseline: 0.4
  mde: 0.03
  min_sample: 4000
decision:
  rule: >-
    Ship if gv_update_viewed sessions reach gv_update_comments_viewed at or
    above the MDE over baseline with no rise in gv_update_notfound; otherwise
    hold.
---

# Project update detail -- read a published progress update

`/governance/updates/:id` renders a single PUBLISHED grant/project progress
update for a project, mirroring the Decentraland governance UI
(`governance-ui src/pages/update.tsx` -> `UpdateDetail`). It is a simple
loader + component surface (no multi-step machine): the loader resolves the
project + its latest published update + the Discourse thread, then the component
composes ui3 `GvProjectUpdateDetail` inside `GovernanceChrome`.

Governance is NOT a Catalyst service (the DAO uses Snapshot + Aragon). The
update, project context, and comments are read LIVE from the upstream public
governance API and ALWAYS fall back to the bundled fixture
`app/fixtures/governance-project-update-detail.json` (itself a live snapshot) so
SSR never breaks:

- `GET /api/updates?project_id=<id>` -- the public-update list (the published
  update is the newest one with health + content; its position is `Update #N`).
- `GET /api/projects` -- header context + the funds-released / disclosed cards
  (derived from the vesting logs + the disclosed `financial_records` sum).
- `GET /api/proposals/<proposal_id>/comments` -- the Discourse comment thread.

The page renders fully without JS (data is in the HTML); analytics fire on the
client. It is READ-ONLY -- there is nothing to simulate.

- `:id` is the **project id** (the `dataSource` is
  `upstream:decentraland/governance/api/updates?project_id=<id>`).
- **Primary metric:** `gv_update_viewed` (a published update was viewed).
- **Guardrails:** `gv_update_comments_viewed` (the Discourse thread scrolled
  into view), `gv_update_notfound` (an unknown id hit the NotFound state).
- **Events:** `gv_update_viewed` on load (real update only),
  `gv_update_comments_viewed` when the comments section is seen,
  `gv_update_notfound` on an unknown id.

Journey steps:

- `/governance/updates/b783aa8f-ebf2-4792-b3eb-8dfccf369dfb` -- the live/fixture
  Protocol Squad "Update #6" with the health banner, all sections, financial
  cards + breakdown, and the Discourse thread.
- `/governance/updates/does-not-exist` -- the `GvNotFound` empty state.

Single shipping variant (`focused-read`); the schema stays fully valid so the
readout tooling and deterministic bucketing work unchanged if a control arm is
added later.
