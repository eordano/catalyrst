/**
 * One honest verdict per admin / operator control.
 *
 * Every entry cites the server-side authorization that was actually read. A
 * control is only `available` when the check exists, was read, and the browser
 * can satisfy it through a path that keeps secrets server-side. Everything else
 * is `unavailable` with a reason the UI renders verbatim -- never a disabled
 * button waiting for a token, never an empty table, never a fixture.
 *
 * This registry is deliberately data, not behaviour: it is the answer to
 * "should this control exist at all", separate from "what did the server say
 * this time", which is the job of the `.server.ts` modules.
 */

import { unavailable, type Unavailable } from "./availability";

export type AvailableVia =
  /** Privileged bearer held in a `.server.ts` module, called from a loader/action. */
  | "server-module"
  /** Genuinely public, unauthenticated endpoint. Must be labelled as public data. */
  | "public-read"
  /** The user's own wallet signature satisfies the gate; no secret involved. */
  | "wallet";

export type ControlAvailable = {
  ok: true;
  via: AvailableVia;
  /** file:line of the server-side check that was read. */
  serverCheck: string;
  /** What the UI must say about the data's provenance. */
  label?: string;
};

export type ControlStatus = ControlAvailable | Unavailable;

const ok = (
  via: AvailableVia,
  serverCheck: string,
  label?: string,
): ControlAvailable => ({ ok: true, via, serverCheck, ...(label ? { label } : {}) });

export const ADMIN_CONTROLS = {

  "places.reports.read": ok(
    "server-module",
    "catalyrst-places/src/handlers/admin.rs:41 -> catalyrst-places/src/auth.rs:88-100",
    "Privileged read via places-moderation.server.ts#loadReportQueue.",
  ),
  "places.reports.decide": ok(
    "server-module",
    "catalyrst-places/src/handlers/admin.rs:83 -> catalyrst-places/src/auth.rs:88-100",
    "Route action only, via places-moderation.server.ts#commitModerationDecision.",
  ),
  "places.place.disable": ok(
    "server-module",
    "catalyrst-places/src/handlers/admin.rs:131 -> catalyrst-places/src/auth.rs:88-100",
    "Route action only, via places-moderation.server.ts#setPlaceDisabled.",
  ),
  "places.list.byOwner": ok(
    "public-read",
    "catalyrst-places/src/handlers/places.rs:66-73 (auth_address_optional, no gate)",
    "Public data \u{2014} no authorization required. `owner` is a filter, not a permission.",
  ),

  "communities.list.read": ok(
    "public-read",
    "catalyrst-social-service/src/rest/handlers/communities.rs:176-177 (try_extract_signer, optional)",
    "Public data \u{2014} no authorization required. Do not label this admin data.",
  ),
  "communities.suspend": ok(
    "server-module",
    "catalyrst-social-service/src/rest/handlers/admin.rs:33-52 require_admin (403 \"admin controls disabled (API_ADMIN_TOKEN unset)\" at :37-42, timing-safe compare at :44); called first by suspend_community:80 and unsuspend_community:112",
    "Route action only, via community-moderation.server.ts#submitSuspension.",
  ),

  "governance.health.read": ok(
    "public-read",
    "catalyrst-governance/src/handlers/health.rs:3 (no auth extractor)",
    "Public data \u{2014} no authorization required.",
  ),
  "governance.budgets.read": ok(
    "public-read",
    "catalyrst-governance/src/handlers/read.rs:220 (no auth extractor)",
    "Public data \u{2014} no authorization required.",
  ),
  "governance.proposal.submit": ok(
    "wallet",
    "catalyrst-governance/src/handlers/write.rs:55-69 verify_signed_fetch (401), :75-84 503 when SnapshotGate::Unconfigured",
    "Signed with the connected wallet. 503 while GOVERNANCE_SUBMIT_URL is unset \u{2014} that is correct.",
  ),

  "admin.metrics.kpis": unavailable(
    "not-wired",
    "No metrics source is wired on this node.",
    {
      serverCheck: null,
      fix:
        "The only live counts available are approved/featured events from the public " +
        "GET /events/api/events?list=all. Everything else needs a real aggregation endpoint.",
    },
  ),

  "whatson.users.savePermissions": unavailable(
    "not-wired",
    "Saving What's-On user permissions is not wired on this node.",
    {
      serverCheck:
        "catalyrst-events/src/handlers/profile_settings.rs:145-155 (federation-envelope preflight + require_moderator)",
      fix:
        "The server gate is real and fails closed. Nothing calls it: " +
        "whatson-admin-users.ts:82-91 declares the types, no caller exists, and the " +
        "signed-envelope builder does not exist.",
    },
  ),

  "debug.tools": unavailable(
    "not-wired",
    "No privileged tooling is wired on this node. The `?authorized=1` parameter was " +
      "never an access control and has been removed.",
    { serverCheck: null },
  ),

  "sceneAdmins.list": unavailable(
    "unreachable",
    "Scene-admin grants cannot be read from this node.",
    {
      serverCheck: "catalyrst-comms/src/handlers/scene_admin.rs:56-62",
      fix:
        "There is no nginx `location` for /scene-admin; the correct public path would " +
        "be /comms/scene-admin and is used nowhere. Adding that edge route is a config " +
        "change, not part of a UI change.",
    },
  ),
  "sceneAdmins.grant": unavailable(
    "unreachable",
    "Granting scene admin is not reachable from this node.",
    {
      serverCheck:
        "catalyrst-comms/src/handlers/scene_admin.rs:123-131 -> ports/scene_perms.rs:16-114 (denies on pool failure :27-34)",
      fix: "Same missing edge route as sceneAdmins.list. The server-side check itself is correct.",
    },
  ),
  "sceneAdmins.revoke": unavailable(
    "unreachable",
    "Revoking scene admin is not reachable from this node.",
    {
      serverCheck:
        "catalyrst-comms/src/handlers/scene_admin.rs:145-157 -> ports/scene_perms.rs:16-114",
      fix: "Same missing edge route as sceneAdmins.list.",
    },
  ),

  "sceneBans.list": unavailable(
    "unreachable",
    "Scene bans cannot be read from this node.",
    {
      serverCheck: "catalyrst-comms/src/handlers/scene_bans.rs:88-89 verify_signed_fetch",
      fix: "Same missing edge route as sceneAdmins.list.",
    },
  ),
  "sceneBans.ban": unavailable(
    "unreachable",
    "Banning in a scene is not reachable from this node.",
    {
      serverCheck:
        "catalyrst-comms/src/handlers/scene_bans.rs:170-178 -> ports/scene_perms.rs:16-114",
      fix: "Same missing edge route as sceneAdmins.list.",
    },
  ),
  "sceneBans.unban": unavailable(
    "unreachable",
    "Unbanning in a scene is not reachable from this node.",
    {
      serverCheck:
        "catalyrst-comms/src/handlers/scene_bans.rs:193-205 -> ports/scene_perms.rs:16-114",
      fix: "Same missing edge route as sceneAdmins.list.",
    },
  ),

  // FIX-FIRST: real gate, client call is wrong. Not implemented.

  "whatson.users.read": unavailable(
    "misrouted",
    "The What's-On moderator list cannot be read: the client call is misrouted.",
    {
      serverCheck:
        "catalyrst-events/src/handlers/profile_settings.rs:40-41 require_auth + authority::require_moderator; catalyrst-fed/src/authority.rs:5-11 (SELECT 1 FROM moderators WHERE address=$1), :13-19 403",
      fix:
        "Three named changes, none applied here: (1) whatson-admin-users.ts:62 requests " +
        "/events/api/events/api/profiles/settings, correct is /events/api/profiles/settings; " +
        "(2) the loader passes no browser identity, so it 401s before it can 403; " +
        "(3) it must sign the UN-prefixed /api/profiles/settings \u{2014} nginx `location /events/api/` " +
        "does not set x-original-path, the opposite convention from /comms/.",
    },
  ),

  "events.moderation.queue": unavailable(
    "not-configured",
    "Event moderation is not configured on this node (CATALYRST_EVENTS_ADMIN_TOKEN unset).",
    {
      status: 503,
      serverCheck:
        "catalyrst-events/src/handlers/events.rs:512 -> catalyrst-events/src/admin.rs:34-44 (403 \"Admin operations are disabled\" when the token is None)",
      fix:
        "Needs a `.server.ts` module holding CATALYRST_EVENTS_ADMIN_TOKEN plus the token " +
        "itself. Not built here: the gate lists this as FIX-FIRST, and provisioning the " +
        "token is a separate, deliberate act.",
    },
  ),
  "events.moderation.decide": unavailable(
    "not-configured",
    "Approving, rejecting and featuring events is not available on this node.",
    {
      status: 503,
      serverCheck:
        "catalyrst-events/src/handlers/events.rs:657 -> catalyrst-events/src/admin.rs:34-44",
      fix:
        "Same as events.moderation.queue. Note that the previous default was " +
        "`simulateModerateAction` " +
        "(features/src/stories/landings/whatson-admin-moderate/machine.ts:169,263), " +
        "which reported success for a decision that never left the browser. That must " +
        "become this unavailable state, not a fake success.",
    },
  ),

  "userBans.list": unavailable(
    "misrouted",
    "The active user-ban list cannot be read: the request is signed over the wrong path.",
    {
      status: 401,
      serverCheck:
        "catalyrst-comms/src/handlers/user_bans.rs:241 -> moderator.rs:65-116 authorize_moderator (MODERATOR_TOKEN bearer OR signer in moderator_addresses; rejects the scene signer)",
      fix:
        "The wallet allowlist IS populated (deploy/env/catalyrst-comms.env:51), so this " +
        "gate is live and wallet-satisfiable. But user-bans.ts:135-139 passes " +
        "`signPath: path` \u{2014} the un-prefixed /users/0x\u{2026}/bans \u{2014} while nginx sets " +
        "x-original-path on /comms/ (01-catalyst.conf:127) and catalyrst-crypto " +
        "signed_fetch.rs:119-134 verifies the PREFIXED value (proven by its own test at " +
        ":671-687). Signing must become `${COMMS_PREFIX}${path}`, conditional on the base " +
        "being the nginx edge. Not applied here: it must be observed returning 200 with a " +
        "wallet from PLATFORM_USER_MODERATORS first. Do NOT put MODERATOR_TOKEN in sites.env.",
    },
  ),
  "userBans.ban": unavailable(
    "misrouted",
    "Banning a user is not available: the request is signed over the wrong path.",
    {
      status: 401,
      serverCheck:
        "catalyrst-comms/src/handlers/user_bans.rs:95-102 -> moderator.rs:65-116",
      fix: "Same signing fix as userBans.list (user-bans.ts:233-263).",
    },
  ),
  "userBans.unban": unavailable(
    "misrouted",
    "Unbanning a user is not available: the request is signed over the wrong path.",
    {
      status: 401,
      serverCheck:
        "catalyrst-comms/src/handlers/user_bans.rs:164-171 -> moderator.rs:65-116",
      fix: "Same signing fix as userBans.list (user-bans.ts:233-263).",
    },
  ),
  "userBans.warn": unavailable(
    "misrouted",
    "Warning a user is not available: the request is signed over the wrong path.",
    {
      status: 401,
      serverCheck:
        "catalyrst-comms/src/handlers/user_bans.rs:212-219 -> moderator.rs:65-116",
      fix: "Same signing fix as userBans.list (user-bans.ts:233-263).",
    },
  ),
} as const satisfies Record<string, ControlStatus>;

export type ControlId = keyof typeof ADMIN_CONTROLS;

export function controlStatus(id: ControlId): ControlStatus {
  return ADMIN_CONTROLS[id];
}

export function isControlAvailable(id: ControlId): boolean {
  return ADMIN_CONTROLS[id].ok === true;
}

/** Every control this node cannot offer, for an "unavailable controls" panel. */
export function unavailableControls(): Array<{ id: ControlId; status: Unavailable }> {
  const out: Array<{ id: ControlId; status: Unavailable }> = [];
  for (const [id, status] of Object.entries(ADMIN_CONTROLS)) {
    if (!status.ok) out.push({ id: id as ControlId, status });
  }
  return out;
}
