---
id: landings-event-subscriptions
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A focused, single-purpose "subscribe to event notifications" flow (sign in ->
    confirm email -> one tap to opt in) lifts the share of visitors who arrive on
    the email-settings surface and actually complete an event-notification
    subscription, versus dropping them into the full multi-category settings page.
  because: >-
    Most landing/event visitors want one thing -- to be told when an event they
    care about starts. Collapsing nine notification accordions into an explicit
    gated wizard (sign-in -> edit -> subscribe -> manage) removes the choice
    overload that makes people bounce before saving anything.
metric:
  primary: landings_subscription_complete_rate
  numerator: landings_subscription_subscribed
  denominator: landings_subscription_started
  guardrails:
    - landings_subscription_started
    - landings_subscription_signin_required
    - landings_subscription_error
decision:
  rule: >-
    Ship if landings_subscription_complete_rate improves by at least the MDE over
    baseline with no guardrail regression (start volume holds, the sign-in gate
    stays graceful, and the commit error rate does not rise); otherwise hold.
    The subscribe/unsubscribe commit is a real signed PUT /subscription, so the
    readout covers durable opt-in state as well as the funnel.
experiment:
  key: landings_event_subscriptions
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
baseline: 0.35
mde: 0.05
min_sample: 4000
---

# Subscribe to event notifications

The event-subscriptions surface (`/landings/event-subscriptions`) turns the
account email-notification settings into a single legible flow: sign in, edit
which categories email you (Events is pre-focused), subscribe, and later manage
or unsubscribe. It is an XState wizard so every step is URL-addressable and the
funnel is measurable.

- **Primary metric:** `landings_subscription_complete_rate` =
  `landings_subscription_subscribed` / `landings_subscription_started`.
- **Guardrails:** start volume (`landings_subscription_started`), a graceful
  sign-in gate (`landings_subscription_signin_required`), and the commit error
  path (`landings_subscription_error`).
- **Events (per transition):** `landings_subscription_started` (idle -> gate),
  `landings_subscription_signin_required` (entering the gate),
  `landings_subscription_signed_in` (gate -> editing),
  `landings_subscription_edited` ({type,enabled} toggles),
  `landings_subscription_submitting` (editing -> submitting),
  `landings_subscription_subscribed` (commit ok),
  `landings_subscription_unsubscribing`,
  `landings_subscription_unsubscribed` (commit ok),
  `landings_subscription_error` (commit failed).

## Data reality

- `GET /events/api/profiles/me/settings` (catalyrst-events
  `handlers/profile_settings.rs::get_auth_profile_settings`) requires an
  auth-chain signed-fetch (anonymous SSR gets 401) and returns
  `subscriptions: []` (web-push deprecated). The loader attempts it best-effort;
  an anonymous or failing read renders the signed-out shell with empty settings.
- The commit is **real**: `buildSubscriptionCommit`
  (`lib/catalyst/landings/subscriptions.ts`) signed-GETs `/subscription`,
  mutates the returned `message_type` map, and signed-PUTs it back. Both routes
  are served by catalyrst-notifications (`src/lib.rs`), which upserts the
  `subscriptions` row. A non-2xx PUT throws -- the wizard never reports a success
  it did not get. Without an identity the commit refuses before any request.
- Email **confirmation** is still unavailable: `PUT /set-email` needs SendGrid
  credentials that are not configured, so `emailConfirmed` stays false. A user
  can set notification preferences but cannot verify an address.
- `POST` / `DELETE /events/api/profiles/subscriptions`
  (`handlers/profile_subscription.rs`) return **410 Gone** -- those are the
  deprecated web-push endpoints, not this flow.
- The category groups + notification types mirror `decentraland/account`
  `src/modules/subscription/utils.ts` (`subscriptionGroups`) and
  `decentraland/schemas` `NotificationType` / `SubscriptionDetails`.
