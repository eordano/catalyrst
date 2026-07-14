---
id: landings-report-abuse
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided, multi-step report-a-player form (target -> category -> details ->
    evidence -> review) increases the share of started reports that reach the
    review step and submit, versus a single long scroll form.
  because: >-
    Breaking the abuse report into legible, one-thing-per-step chunks reduces the
    cognitive load of recalling the incident and gathering evidence, so more
    people who start a report push through to submit instead of abandoning a
    daunting wall of fields.
metric:
  primary: report_submit_rate
  numerator: report_completed
  denominator: report_started
  guardrails:
    - report_started
    - report_validation_failed
    - report_evidence_added
decision:
  rule: >-
    Ship if report_submit_rate improves by at least the MDE with no guardrail
    regression (report-start volume holds, validation-failure rate does not rise,
    and evidence attachment stays healthy); otherwise hold. Submission is a
    SIMULATED stub here, so the readout is the in-product funnel up to submit.
experiment:
  key: landings_report_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
        requireConfirm: true
  baseline: 0.45
  mde: 0.05
  min_sample: 3000
---

# Report a player / abuse (submission stubbed)

The report-abuse flow (`/landings/report-abuse`) turns decentraland.org's
single-card `/report` form into an explicit multi-step wizard:

1. **intro** -- what reporting is for + the sign-in / wallet identity gate.
2. **target** -- the reported user's wallet address (+ your own, tied to identity).
3. **category** -- the reason (Scam/Phishing, Illegal Content, Harassment,
   Cheating, Impersonation).
4. **details** -- a free-text description of what happened.
5. **evidence** -- up to 5 screenshots/videos/PDFs.
6. **review** -- confirm accuracy and submit.
7. **submitting** -- presign -> upload each file -> finalize, all signed-fetch.
8. **success** -- the confirmation card.

This story tracks whether the wizard increases the share of started reports that
reach review and submit.

- **Primary metric:** `report_submit_rate` = `report_completed` / `report_started`.
- **Guardrails:** report-start volume (`report_started`), validation-failure rate
  (`report_validation_failed`), and evidence attachment (`report_evidence_added`)
  must stay healthy.
- **Events:** `report_started` (entering target), `report_target_set`
  (`{has_reporter}`), `report_category_set` (`{reason}`), `report_details_set`
  (`{description_len}`), `report_evidence_added` (`{file_count}`),
  `report_review_reached`, `report_validation_failed` (`{fields}`),
  `report_submit_started`, `report_completed` (`{report_id}`), `report_failed`
  (`{reason}`).

## Data reality

The submission is **real**. `buildSubmitReport(identity)`
(`app/lib/catalyst/landings/report.ts`) runs the upstream three-step wire against
`catalyrst-comms`: `POST /reports/players/presign` -> `PUT` each evidence file to
the returned `uploadPath` -> `POST /reports/players`, every call signed-fetch
under the `/comms` prefix (signed on the unprefixed route). The server binds the
reporter to the signature and ignores any client-supplied `playerAddress`;
evidence bytes land in `player_report_evidence` and moderators read the queue at
`GET /reports`. Without a connected wallet the wizard fails closed --
`failClosedSubmitReport` throws rather than showing a success card.

The form shape + copy are faithful to the upstream report feature.
