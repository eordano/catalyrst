---
id: admin-communities-moderation
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A dedicated global-moderator review surface (search + status filter ->
    review a flagged community -> suspend with a reason / unsuspend) increases
    the share of opened community reviews that reach a committed
    suspend/unsuspend decision, versus moderators acting from raw admin lists.
  because: >-
    Surfacing the flag reason, owner, privacy and member count next to a single
    Suspend-with-reason / Unsuspend control removes the context-gathering and
    tooling friction that makes moderators open a community, hesitate, and bail
    without acting -- so more reviews convert to a decisive, auditable commit.
metric:
  primary: admin_community_review_to_commit_rate
  numerator: admin_community_suspension_committed
  denominator: admin_community_reviewed
  guardrails:
    - admin_community_reviewed
    - admin_community_moderation_failed
experiment:
  key: admin_communities_moderation
  unit: session
  variants:
    - id: moderation_console
      weight: 1
      flags:
        moderation_console: true
  baseline: 0.45
  mde: 0.05
  min_sample: 2000
decision:
  rule: >-
    Ship if admin_community_review_to_commit_rate improves by at least the MDE
    with no guardrail regression (review volume holds and the moderation-failure
    rate stays flat); otherwise hold.
---

# Moderate communities (suspend / unsuspend SIMULATED)

The communities moderation console (`/admin/communities-moderation`) gives a
global moderator a review queue from
`GET /v1/moderation/communities?search=&limit=&offset=`
(catalyrst-communities `handlers/moderation.rs get_moderation_communities`,
`global_moderators`-gated -> 403). A search box and `all | active | suspended |
inactive` status pills filter the list (cross-referenced with the bearer-gated
`GET /v1/admin/communities?status=&owner=&search=` admin list, the authoritative
source of suspension state). Selecting a community opens a review card -- name,
owner, privacy, member count and the flagged reason -- over a Suspend-with-reason
/ Unsuspend decision bar.

The commit is SIMULATED: `POST /v1/admin/communities/{id}/suspend {reason}` and
`/unsuspend` (`handlers/admin.rs`, `API_ADMIN_TOKEN` bearer, FAIL-CLOSED 403 when
the token env is unset). There is no admin token in an anonymous browser session,
so the `submitting` state runs a simulated actor, never a real `POST`. Flow,
states, telemetry and the `SuspendBody` are REAL; the admin bearer is simulated.

- **Primary metric:** `admin_community_review_to_commit_rate` =
  `admin_community_suspension_committed` / `admin_community_reviewed`.
- **Guardrails:** review volume (`admin_community_reviewed`) and the
  moderation-failure rate (`admin_community_moderation_failed`).
- **Events:** `experiment_exposed` on render, `admin_community_list_viewed`
  (`{ total, status_filter }`), `admin_community_reviewed` (`{ community_id }`),
  `admin_community_decision_selected` (`{ community_id, decision }`),
  `admin_community_suspension_committed` (`{ community_id, suspended, has_reason }`),
  `admin_community_moderation_failed` (`{ community_id }`).

Data reality: the `/v1/moderation/communities` and `/v1/admin/communities` routes
are not proxied on live catalyst (404 via nginx), and an anonymous browser is not
a global moderator (403) nor carries an admin bearer (403). The review list reads
live-FIRST and degrades to the bundled fixture; the suspend/unsuspend commit is
simulated. Noted as deferred.
