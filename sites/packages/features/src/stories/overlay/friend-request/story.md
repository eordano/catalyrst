---
id: bevy-overlay-friend-request
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A legible, explicit friend-request flow (send -> accept/cancel -> confirmation,
    with block as a guarded two-step) increases the share of opened Friends
    panels that complete a friendship action, even with the final RPC simulated.
  because: >-
    Friendships travel over an opaque WebSocket RPC the browser can't reach, so
    surfacing each upsert_friendship transition (request/accept/cancel/reject/
    block) as a distinct, URL-addressable step makes the available action obvious
    and reversible, so more sessions that open the panel push an action through
    to RequestOperationConfirmed instead of bailing at an ambiguous icon.
metric:
  primary: ov_friend_action_completion_rate
  numerator: ov_friend_action_completed
  denominator: ov_friend_panel_opened
  guardrails:
    - ov_friend_panel_opened
    - ov_friend_block_confirmed
experiment:
  key: ov_friend_request
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.35
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if ov_friend_action_completion_rate improves by at least the MDE with no
    guardrail regression (panel-open volume holds and block stays a deliberate
    confirmed two-step, i.e. ov_friend_block_confirmed does not spike relative to
    block attempts); otherwise hold.
---

# Bevy overlay -- Send / accept / cancel a friend request and block a user

From the loaded HUD, opening the Friends panel (`?panel=friends`) surfaces the
Friends / Requests / Blocked tabs. From a request row or a profile passport the
user can run an `upsert_friendship` action: **request**, **accept**,
**cancel**/**reject**, or **block** (a guarded two-step confirmation). Each step
is URL-addressable so the journey can be screenshotted without a browser script.

Data reality: friendships are **RPC-over-WebSocket only**. `catalyrst-social-rpc`
exposes a single HTTP route (`/`) that is a WebSocket upgrade -- the friendship
RPCs (`GetFriends`, `GetPending/SentFriendshipRequests`, `UpsertFriendship`,
`GetBlockedUsers`) are NOT reachable over plain HTTP GET. In the live client they
arrive via `window.dclBridge` (`GetFriends`/`GetOnlineFriends`). The bridge is
absent here, so the panel renders the seed shapes in
`app/fixtures/bevy-overlay-friend-request.json` (derived faithfully from the
`decentraland.social_service.v2` proto) and the **final `UpsertFriendship` RPC is
SIMULATED** via an injectable bridge stub. The flow / states / `transition_valid`
state machine / metrics are all real.

Journey (each step a real URL):

1. `?panel=friends` -- open the Friends panel (Friends / Requests / Blocked tabs).
2. `?panel=friends&tab=requests` -- view Received / Sent requests.
3. `?panel=friends&action=add&address=<addr>` -- send a friend request
   (`UpsertFriendship` `request`).
4. `?panel=friends&action=accept&address=<addr>` -- accept a received request
   (`UpsertFriendship` `accept`).
5. `?panel=friends&action=cancel&address=<addr>` -- cancel a sent request / reject
   (`UpsertFriendship` `cancel`|`reject`).
6. `?panel=friends&action=block&address=<addr>` -- block confirmation
   (`BlockUserPrompt`) then `UpsertFriendship` `block`.
7. `?panel=friends&action=done` -- confirmation (`RequestOperationConfirmed`).

Events (`app/lib/telemetry/track.ts`):

- `ov_friend_panel_opened` -- panel mount (`{ tab }`).
- `ov_friend_action_started` -- entering a confirm step (`{ action, address }`).
- `ov_friend_block_prompt` -- entering the block confirmation (`BlockUserPrompt`).
- `ov_friend_block_confirmed` -- block confirmed (guardrail).
- `ov_friend_action_completed` -- `RequestOperationConfirmed` reached
  (`{ action, address, stub: true }`).
- `ov_friend_action_failed` -- simulated RPC rejection / invalid transition.

Primary metric `ov_friend_action_completion_rate` =
`ov_friend_action_completed` / `ov_friend_panel_opened`.
