---
id: admin-places-moderation
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A status-bucketed report queue with an explicit review -> decision flow
    (resolve | dismiss | action+disable | reopen) increases the share of opened
    place reports that reach a recorded decision, versus an unstructured list.
  because: >-
    Surfacing the reporter, the reported place (coords + thumbnail) and the
    reason on one review panel, then forcing a single explicit decision with a
    resolution note, removes ambiguity about what action to take -- so moderators
    push opened reports through to a logged resolution instead of leaving them
    open.
metric:
  primary: admin_place_moderation_decision_rate
  numerator: admin_place_moderation_committed
  denominator: admin_place_report_opened
  guardrails:
    - admin_place_report_opened
    - admin_place_moderation_failed
experiment:
  key: admin_place_moderation_queue
  unit: session
  variants:
    - id: bucketed_queue
      weight: 1
      flags:
        bucketed_queue: true
  baseline: 0.5
  mde: 0.05
  min_sample: 2000
decision:
  rule: >-
    Ship if admin_place_moderation_decision_rate (committed / opened) improves by
    at least the MDE with no guardrail regression (report-open volume holds and
    the commit failure path stays rare); otherwise hold.
---

# Admin -- Places report moderation queue

The Places moderation console (`/admin/places-moderation`) lets an admin work the
report queue: an open / resolved / dismissed / actioned bucketed grid, a review
panel (reporter, reported place coords + thumbnail, reason), and a decision bar
(Resolve | Dismiss | Action+Disable place | Reopen, plus a resolution note). This
story tracks whether the structured queue increases the share of opened reports
that reach a recorded decision.

- **Primary metric:** `admin_place_moderation_decision_rate` =
  `admin_place_moderation_committed` / `admin_place_report_opened`.
- **Guardrails:** report-open volume (`admin_place_report_opened`) and the
  commit failure path (`admin_place_moderation_failed`) must stay healthy.
- **Events:** `experiment_exposed`, `admin_place_queue_viewed`
  (`{open_count, total}`), `admin_place_report_opened` (`{report_id, entity_id}`),
  `admin_place_decision_selected` (`{report_id, decision}`),
  `admin_place_disable_toggled` (`{place_id, disabled}`),
  `admin_place_moderation_committed` (`{report_id, decision, place_disabled}`),
  `admin_place_moderation_failed` (`{report_id, reason}`).

Data reality: every call on this page is real, and every one of them is
admin-bearer gated server-side. `gate()`
(`catalyrst/crates/catalyrst-places/src/handlers/admin.rs:13-15`) delegates to
`require_admin_bearer` (`catalyrst-places/src/auth.rs:88-100`), which answers
403 "Admin token not configured" when `PLACES_ADMIN_AUTH_TOKEN` is unset and
403 "Invalid admin credentials" otherwise; the compare is timing-safe. `gate()`
is the first statement of `get_reports` (`admin.rs:41`), `patch_report`
(`:83`) and `patch_place_disable` (`:131`).

Both directions run server-side only. The queue read is
`places-moderation.server.ts#loadReportQueue`, called from the route loader.
The commit is `PATCH /places/api/reports/{id}` plus an optional
`PATCH /places/api/places/{place_id}/disable`, issued by
`places-moderation.server.ts#commitModerationDecision` from the
`/admin/places-decision` route action. The bearer never enters the browser
bundle.

Nothing here is simulated. An earlier version of this file said the commit was
SIMULATED; that was false -- the machine's default `moderate` was the real write
path, which read `process.env.PLACES_ADMIN_AUTH_TOKEN` in browser code (always
undefined), omitted the `authorization` header and sent the PATCH anyway. That
client write path has been deleted. `simulateModerateReport` is still exported
for stories and tests, and is used nowhere in the app.

There is also no client-side gate. The old auth-gate state, with its
unconditional `SIGN_IN -> queue` transition behind an "Open moderation console"
button, checked nothing; it has been removed. The console renders only when the
loader's server-side read answered ok, and renders the server's reason
otherwise.

`PLACES_ADMIN_AUTH_TOKEN` is set in no env file on this node, so today the page
renders "not configured on this node" for every visitor. That is the correct,
fail-closed outcome; provisioning the token is a separate, deliberate act.
