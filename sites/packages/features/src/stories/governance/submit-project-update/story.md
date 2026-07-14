---
id: governance-submit-project-update
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, deep-linkable "Publish a Grant/Project Update" wizard (general ->
    financials -> preview -> publish -> success) increases the share of started
    grant-update drafts that reach the publish/confirm step, versus the long
    single-scroll governance update form.
  because: >-
    Splitting the dense update into legible steps -- project health + the five
    markdown fields first, then the funds-released / disclosed financials and the
    reporting CSV, then an explicit preview before publishing -- reduces drop-off
    from form fatigue, so more grantees who start a draft push through to
    publishing instead of abandoning the wall of fields.
metric:
  primary: gv_update_publish_rate
  numerator: gv_update_publish_attempted
  denominator: gv_update_started
  guardrails:
    - gv_update_started
    - gv_update_financials_set
experiment:
  key: gv_project_update_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.4
  mde: 0.05
  min_sample: 2500
decision:
  rule: >-
    Ship if gv_update_publish_rate improves by at least the MDE with no guardrail
    regression (update-draft start volume holds and authors still reach the
    financials step); otherwise hold.
---

# Publish a Grant/Project Update (multi-step write flow, simulated)

The project-update wizard (`/governance/projects/:id/update`) breaks the DAO's
dense "Publish New Grant Update" submission into explicit, deep-linkable steps:

1. `?step=general` -- project health (On Track / At Risk / Off Track) plus the
   five markdown fields (Introduction, Highlights, Blockers, Next Steps,
   Additional notes and links) with their server maxLengths
   (`GeneralUpdateSectionSchema`: 500 / 3500 / 3500 / 3500 / 3500).
2. `?step=financials` -- funds-released / funds-disclosed cards (projected from the
   project's **live** vesting), the reporting CSV textarea, and a disclosed
   summary.
3. `?step=preview` -- preview the assembled update before publishing.
4. `?step=publishing` -- **simulated** `createUpdate` (stub).
5. `?step=success` -- the `UpdateSuccessModal` outcome screen
   (`GvProposalDetailSuccessOutcomeScreens` variant `update`, the `?newUpdate`
   analog).

- **Primary metric:** `gv_update_publish_rate` = `gv_update_publish_attempted` /
  `gv_update_started`.
- **Guardrails:** update-draft start volume (`gv_update_started`) and financials
  completion (`gv_update_financials_set`) must stay healthy.
- **Events:** `gv_update_started` (`{health}`), `gv_update_financials_set`
  (`{disclosed, records}`), `gv_update_previewed`, `gv_update_publish_attempted`,
  `gv_update_published` (`{update_id, project_id, simulated:true}`).

**Data reality / simulated:** Governance is NOT a Catalyst service -- the DAO runs
on Snapshot + Aragon externally and there is no update-submission write API. The
project context (header, vesting/funding cards, prior public updates) is read
LIVE from the upstream governance API
(`governance.decentraland.org/api/projects` + `/api/updates?project_id=<id>`),
with a fixture fallback in `app/fixtures/governance-submit-project-update.json`
(itself derived from a live snapshot of the "Protocol Squad -- Alternative
Explorers" grant). The final `createUpdate` is SIMULATED in the XState machine (a
stubbed resolver, never a real POST). The flow, states, and metrics are real;
only the on-chain/server commit is a clearly-noted stub.
