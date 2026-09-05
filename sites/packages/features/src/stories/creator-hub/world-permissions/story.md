---
id: creator-hub-world-permissions
status: draft
owner: owner@example.com
hypothesis:
  statement: >-
    A guided World-permissions wizard (review access type -> invite addresses ->
    set a password -> manage collaborators -> confirm) increases the share of
    started permission edits that reach the confirm/ACL-write step, even with the
    write simulated.
  because: >-
    Splitting access control into explicit, legible steps (who can enter, how
    they prove it, who can build) reduces the fear of locking yourself out, so
    more owners who open the permissions modal push through to actually saving a
    change instead of bailing at an opaque all-in-one form.
metric:
  primary: ch_world_perms_confirm_rate
  numerator: ch_world_perms_confirm_reached
  denominator: ch_world_perms_started
  guardrails:
    - ch_world_perms_started
    - ch_world_perms_invalid_address
experiment:
  key: ch_world_perms_wizard
  unit: session
  variants:
    - id: wizard
      weight: 1
      flags:
        wizard: true
  baseline: 0.45
  mde: 0.05
  min_sample: 4000
decision:
  rule: >-
    Ship if ch_world_perms_confirm_rate improves by at least the MDE with no
    guardrail regression (edit-start volume holds and the invalid-address rate
    does not climb); otherwise hold.
---

# Manage a World's access + collaborators (ACL write simulated)

The World Permissions wizard (`/creator-hub/world-permissions`) walks a World
owner through access control in explicit steps:

1. **access** -- review the current access type (Public / Invitation only /
   Password protected) and the approved-addresses list.
2. **invite** -- the New-Invite form (Wallet / Community / CSV tabs).
3. **password** -- set or change the access password (the `shared-secret` type;
   min 8 chars, >= 2 numbers, must match).
4. **collaborators** -- the Collaborators tab list (deploy / stream scope).
5. **add-collaborator** -- the Add Collaborator dialog; validates a `0x...`
   address (0x + 40 hex) before it can be confirmed.
6. **confirm** -- a SIMULATED ACL write (the real worlds-content-server route is
   `POST /world/:world_name/permissions/access` + `PUT
   /world/:world_name/permissions/deployment/:address`).

- **Primary metric:** `ch_world_perms_confirm_rate` =
  `ch_world_perms_confirm_reached` / `ch_world_perms_started`.
- **Guardrails:** edit-start volume (`ch_world_perms_started`) and the
  invalid-address rate (`ch_world_perms_invalid_address`) must stay healthy.
- **Events:** `ch_world_perms_started` (entering invite from access),
  `ch_world_perms_access_type_set` (`{access_type}`),
  `ch_world_perms_invite_submitted` (`{channel}`),
  `ch_world_perms_password_set`, `ch_world_perms_collaborator_validated`
  (`{valid}`), `ch_world_perms_invalid_address`,
  `ch_world_perms_confirm_reached`, `ch_world_perms_completed` (stub).

Data reality: Worlds permissions are NOT exposed by the public catalyst
(`/worlds/*` and the worlds-content-server permissions API 404 from this
loader), so the ACL is seeded from the upstream
`decentraland/worlds-content-server` shapes
(`logic/access/types.ts` `AccessSetting`, `logic/permissions/types.ts`
`AllowListPermissionSetting`, `permissions-handlers.ts` GET response) into
`app/fixtures/creator-hub-world-permissions.json`. The final ACL write is
**simulated** (no real POST/PUT) -- flow, states, validation and metrics are
real; the commit is a clearly-noted stub. Noted as deferred.
