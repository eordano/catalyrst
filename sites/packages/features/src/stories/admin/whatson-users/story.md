---
id: admin-whatson-users
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A dedicated What's-On admin users screen -- one table of every wallet on the
    allow-list with its permission checkmarks, plus a single add/edit-roles
    modal -- increases the share of admins who open the permission editor and
    save a change, versus managing the allow-list out of band.
  because: >-
    Showing the whole allow-list and each wallet's four permissions at a glance,
    then collapsing add and edit into one modal with per-permission switches and
    an explicit Save, removes the ambiguity of who can do what -- so admins
    actually open the editor and commit a permission change instead of leaving
    the allow-list stale.
metric:
  primary: admin_users_unavailable_viewed
  guardrails: []
experiment:
  key: admin_whatson_users
  unit: session
  variants:
    - id: users_table
      weight: 1
      flags:
        users_table: true
  baseline: 0.5
  mde: 0.05
  min_sample: 1500
decision:
  rule: >-
    Not shippable as an experiment until the read path is fixed and the write
    path exists. The readout below is retained as the intended design.
---

# Admin -- What's On admin users & permissions

`/admin/whatson-users` currently renders **no table and no editor**. It renders
what the server would say, and why this side cannot ask it.

## `?admin=1` is gone

The loader used to read `?admin=1` and turn it into a `hasAdminScope` boolean
handed to the data layer -- a query parameter the visitor sets, standing in for
an authorization decision. That is the pretend-gate this build exists to remove.

The real gate is server-side and needs no secret at all, only the caller's own
wallet:

- `catalyrst-events/src/handlers/profile_settings.rs:40-41` --
  `require_auth(&headers, "get", "/api/profiles/settings")`, then
  `authority::require_moderator`.
- `catalyrst-fed/src/authority.rs:5-11` -- `SELECT 1 FROM moderators WHERE
  address = $1`; `:13-19` -- 403 otherwise.

When the read is fixed the page will show that server's answer to the connected
wallet -- 200, 403 "not a moderator on this node", or 401 "not connected" -- and
will not simulate any of them locally.

## Read -- FIX-FIRST, three named defects

None is fixed here; the build gate classifies this as FIX-FIRST.

1. `whatson-admin-users.ts:62` requested
   `/events/api/events/api/profiles/settings`. The `/api` segment is doubled;
   the correct path is `/events/api/profiles/settings`.
2. The loader passed no browser identity, so the call 401s before it can 403 --
   "not a moderator" and "not connected" were indistinguishable.
3. It must sign the **un-prefixed** `/api/profiles/settings`. nginx
   `location /events/api/` (`01-catalyst.conf:81`) does not set
   `x-original-path` and `_proxy.inc` does not add it, so `signed_fetch_path`
   falls back to the route path. This is the opposite convention from `/comms/`
   -- get it wrong and it 401s.

The previous implementation also swallowed every failure into
`{ rows: [], source: "empty" }`, so a 403 rendered as an empty allow-list. "You
may not see this" and "there is nobody on the list" looked identical.

## Write -- BLOCK

`catalyrst-events/src/handlers/profile_settings.rs:145-155` does a
federation-envelope preflight and then `require_moderator`. The check is real
and fails closed. Nothing on this side reaches it: the types in
`whatson-admin-users.ts:82-91` are declared and have no caller, and the
signed-envelope builder the endpoint requires does not exist in this repo.

So Save renders as a disabled control carrying that reason, not as a button that
cannot save. `AdminUsersTable` -- whose only job was to attach
`admin_user_permissions_saved` telemetry to a modal that saved nothing -- has
been deleted, along with `admin_users_viewed`,
`admin_user_permission_modal_opened` and `admin_user_permissions_saved`. ui3's
`StWhatSOnAdminUsers` is untouched and remains a Storybook component.

- **Events:** `experiment_exposed`, and
  `admin_users_unavailable_viewed { reason }` on mount.
