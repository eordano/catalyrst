---
id: governance-edit-project-update
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Editing an existing Project Update through a guided, prefilled, deep-linkable
    wizard (general -> financials -> confirm-on-save) increases the share of
    started edits that reach the save/confirm step, versus the single long
    governance edit form that publishes immediately.
  because: >-
    Prefilling the health + markdown fields and the financial section from the
    update being edited, then gating the overwrite behind an explicit
    EditUpdateModal ("saving updates previously published content"), reduces both
    form fatigue and fear of clobbering a published update -- so more grantees who
    start an edit push through to saving instead of abandoning a dense,
    immediately-destructive form.
metric:
  primary: gv_update_edit_save_rate
  numerator: gv_update_edit_save_attempted
  denominator: gv_update_edit_started
  guardrails:
    - gv_update_edit_started
    - gv_update_edit_financials
experiment:
  key: gv_update_edit_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.4
  mde: 0.05
  min_sample: 3000
decision:
  rule: >-
    Ship if gv_update_edit_save_rate improves by at least the MDE with no
    guardrail regression (edit-start volume holds and editors still reach the
    financials step); otherwise hold.
---

# Edit an existing Project Update (prefilled, confirm-on-save, simulated)

The edit-update wizard (`/governance/projects/:id/update-edit`) reopens an
already-published Project Update prefilled from the existing record and walks the
edit in explicit, deep-linkable steps. Unlike the *submit* flow (which publishes
immediately), edit is **confirm-on-save**: an `EditUpdateModal` asks "Are you
sure you want to edit this update?" before the (simulated) save runs, because
saving overwrites previously published content.

1. `?step=general` -- **prefilled** Project Health toggle (On Track / At Risk /
   Off Track) + five markdown fields (Introduction, Highlights, Blockers, Next
   Steps, Additional notes), each with a live char-counter, all seeded from the
   existing update read **live** from
   `governance.decentraland.org/api/updates?project_id=<id>`.
2. `?step=financials` -- **prefilled** Financials section: funds-released /
   funds-disclosed cards, the Reporting CSV editor + dropzone, and the disclosed
   records Summary table.
3. `?step=confirm` -- `EditUpdateModal` confirm-on-save (instead of immediate
   publish).
4. `?step=saving` -- **simulated** save (stub spinner; no write API).
5. `?step=done` -- saved.

- **Primary metric:** `gv_update_edit_save_rate` =
  `gv_update_edit_save_attempted` / `gv_update_edit_started`.
- **Guardrails:** edit-start volume (`gv_update_edit_started`) and financials
  completion (`gv_update_edit_financials`) must stay healthy.
- **Events:** `gv_update_edit_started` (`{project_id, update_id, health}`),
  `gv_update_edit_financials` (`{records}`), `gv_update_edit_confirm_open`,
  `gv_update_edit_save_attempted`, `gv_update_edit_saved`
  (`{update_id, simulated:true}`).

**Data reality / simulated:** Governance is NOT a Catalyst service -- the DAO runs
on Snapshot + Aragon externally and there is no update-write API. The existing
update is read LIVE from the governance updates API (fixture fallback in
`app/fixtures/governance-edit-project-update.json`, a live snapshot). The chosen
update's `financial_records` was `null` upstream, so the Financials section is
DERIVED from the upstream `FinancialRecordSchema` (decentraland/governance@master
`src/entities/Updates/types.ts`) and marked `_financial_records_derived`. The
final `Governance.updateProjectUpdate` is SIMULATED in the XState machine (a
stubbed resolver, never a real POST). The flow, states, and metrics are real;
only the on-chain/API commit is a clearly-noted stub.
