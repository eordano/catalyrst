---
id: bevy-overlay-notifications
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    Surfacing a category filter and an explicit "mark all read" control in the
    Notifications panel will raise the share of sessions that act on a
    notification (open a card or clear the unread badge), because players who can
    triage by category find the relevant card faster and feel in control of an
    otherwise noisy feed.
  because: >-
    The unfiltered, ever-growing feed buries actionable cards (friend requests,
    gifts) under low-signal system messages; players bounce without engaging. A
    category filter plus a one-tap "mark all read" lets a player jump straight to
    the cards they care about and reach inbox-zero, increasing the rate of
    sessions that complete a notification action.
metric:
  primary: notif_action_rate
  numerator: notif_marked_read
  denominator: notif_panel_opened
  guardrails:
    - notif_panel_opened
    - notif_filter_applied
    - notif_marked_read
    - notif_mark_all_read
decision:
  rule: >-
    Ship the filter + mark-all-read treatment if notif_action_rate is higher than
    control with 95% confidence and neither notif_panel_opened nor
    notif_marked_read regresses beyond tolerance; otherwise keep the plain feed.
experiment:
  key: notif_triage_controls
  unit: session
  variants:
    - id: control
      weight: 50
      flags:
        showFilters: false
        markAll: false
    - id: treatment
      weight: 50
      flags:
        showFilters: true
        markAll: true
  baseline: 0.31
  mde: 0.04
  min_sample: 8200
---

# Bevy overlay -- Open notifications and mark them read

The in-world HUD Notifications panel, opened from the sidebar bell. It lists the
notifications addressed to the signed-in wallet (friends / badge / gift /
community / marketplace / system cards), newest-first, and lets the player filter
by category and mark a card -- or all cards -- read.

## Journey (URL-addressable)

- `/client?panel=notifications` -- open the Notifications panel from the sidebar
  bell. Emits **notif_panel_opened** ({ count, unread }).
- `/client?panel=notifications&filter=<type>` -- filter by category
  (`friends|badge|gift|community|marketplace|system`). Emits
  **notif_filter_applied** ({ filter, count }).
- mark-read action -- mark one card (the per-card check) or all cards
  ("Mark all read") read. Emits **notif_marked_read** ({ id, type }) /
  **notif_mark_all_read** ({ count }), which roll up into the primary
  **notif_action_rate**.

## Data + what is simulated

The feed is served by the `catalyrst-notifications` crate
(`GET /notifications` -> `{ notifications: NotificationItem[] }`,
`PUT /notifications/read`). Both routes require a signed auth-chain header
(`require_signer`); the public edge `https://catalyst.example.com/notifications`
returns a **301 redirect** and is not fetchable unauthenticated. So the loader
attempts the live read best-effort and falls back to the bundled fixture
(`app/fixtures/bevy-overlay-notifications.json`, derived from the
`decentraland/schemas` `NotificationType` enum + the crate `NotificationItem`
shape). The **mark-read PUT is SIMULATED**: the read-state transition is real and
local (the panel reflects it immediately), but the signed `PUT /notifications/read`
is not issued.
