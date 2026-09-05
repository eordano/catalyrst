---
id: operator-user-bans
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A single global ban & warning console (active-ban list + lookup-by-address +
    a guided ban/warn/unban action flow) lets moderators resolve a flagged
    account faster and with fewer mistakes than ad-hoc per-room kicks.
  because: >-
    Surfacing the current active bans, a definitive isBanned lookup, and the
    faithful 409-already-banned / 404-no-active-ban guardrails inline means a
    moderator confirms the right action against the right address before
    committing, so more started actions reach a clean confirm/commit instead of
    bouncing off a duplicate-ban or empty-unban error.
metric:
  primary: operator_user_action_commit_rate
  numerator: operator_user_ban_committed
  denominator: operator_user_action_selected
  guardrails:
    - operator_user_bans_viewed
    - operator_user_ban_failed
experiment:
  key: op_user_bans_console
  unit: session
  variants:
    - id: console
      weight: 1
      flags:
        console: true
  baseline: 0.5
  mde: 0.05
  min_sample: 2000
decision:
  rule: >-
    Not shippable as an experiment until the signed-path defect below is fixed
    AND observed returning 200 from a PLATFORM_USER_MODERATORS wallet. The
    readout below is retained as the intended design.
---

# Scene Operator / Moderator -- Global user ban & warning console

The console at `/operator/user-bans` is the moderator surface for catalyst-comms
GLOBAL user moderation (`/bans`, `/users/{addr}/bans`, `/users/{addr}/warnings`).
It lists the active global bans, lets a moderator look up any address's ban
status, and walks a guided action flow: ban (reason + duration + custom message),
warn, or lift an existing ban -- then confirm and commit.

- **Primary metric:** `operator_user_action_commit_rate` =
  (`operator_user_ban_committed` + `operator_user_warning_committed` +
  `operator_user_unban_committed`) / `operator_user_action_selected`.
- **Guardrails:** the list view event (`operator_user_bans_viewed`) must keep
  firing (the read still degrades gracefully to the fixture) and the faithful
  failure event (`operator_user_ban_failed`) must not regress.
- **Events:** `experiment_exposed`, `operator_user_bans_viewed { active_count }`,
  `operator_user_ban_lookup { is_banned }`,
  `operator_user_action_selected { action: ban|unban|warn }`,
  `operator_user_ban_committed { has_duration }`,
  `operator_user_warning_committed`, `operator_user_unban_committed`,
  `operator_user_ban_failed { action, reason: already_banned|no_active_ban }`.

## Data reality -- BLOCK, and this one is close

`/operator/user-bans` currently renders **no ban list and no action flow**.

This is the one blocked surface whose gate is both real and genuinely
satisfiable by a human operator with no secret at all:

- `catalyrst-comms/src/handlers/user_bans.rs:241` (list)
- `catalyrst-comms/src/handlers/user_bans.rs:95-102`, `:164-171`, `:212-219`
  (ban / unban / warn)
- all -> `catalyrst-comms/src/moderator.rs:65-116` `authorize_moderator`: a
  `MODERATOR_TOKEN` bearer, **or** a signer present in `moderator_addresses`
  (and it explicitly rejects the scene signer).

`PLATFORM_USER_MODERATORS` **is populated** on this node
(`deploy/env/catalyrst-comms.env:51`), so the wallet path is live.

### The defect is on this side, and this document used to state it backwards

An earlier version of this file said: "catalyrst-comms is published under
`/comms` with the prefix stripped and without `x-original-path`, so signed calls
sign the unprefixed route path." That is wrong, and it is why the bug survived.

`01-catalyst.conf:127` **does** set `x-original-path` on `/comms/`, and
`catalyrst-crypto/src/signed_fetch.rs:119-134` rebinds the verified path to that
header, verifying the **prefixed** value -- proven by the crate's own test
`verify_signed_fetch_accepts_proxy_prefixed_original_path`
(`signed_fetch.rs:671-687`), where the client signs `/market/v1/lists`.

`user-bans.ts:135-139` and `:233-263` pass `signPath: path` -- the un-prefixed
`/users/0x.../bans` -- and `signer.ts:74` signs exactly that. Every call 401s.

### Why it is still not wired

The fix is named and small: sign `` `${COMMS_PREFIX}${path}` `` when the base is
the nginx edge (a direct-to-comms base sends no `x-original-path`, where the
un-prefixed form is correct). The build gate requires it to be **observed
returning 200 with a wallet from `PLATFORM_USER_MODERATORS`** before it ships,
and that runtime check has not been performed. So this stays BLOCK and
`user-bans.ts` is unchanged.

What is deliberately **not** done: putting `MODERATOR_TOKEN` in `sites.env`. It
is a server-to-server bearer; the wallet allowlist is the correct gate for a
human operator and needs no secret.

The loader no longer calls `loadActiveBans`. It produced a 401 that the route
swallowed into an empty list, so "nobody is banned" and "you may not see who is
banned" rendered identically. Ban, unban and warn render as disabled controls
carrying the reason, and the page emits
`operator_control_unavailable { control, reason }`.
