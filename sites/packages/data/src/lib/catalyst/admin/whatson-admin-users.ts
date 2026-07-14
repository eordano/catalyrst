import { z } from "zod";

import type { GetOptions } from "../client";
import { apiOkOf } from "../envelope";
import type { ApiOk as RsApiOk } from "@ui/generated/catalyst/events/ApiOk";
import { AdminEntrySchema, type AdminEntry } from "./whatson-admin";
import { controlStatus } from "./control-availability";
import type { ControlResult, Unavailable } from "./availability";

export const ADMIN_PERMISSIONS = [
  "approve_own_event",
  "approve_any_event",
  "edit_any_event",
  "edit_any_profile",
] as const;
export type AdminPermission = (typeof ADMIN_PERMISSIONS)[number];

export type AdminUserRow = {
  user: string;
  name: string | null;
  permissions: string[];
  hue: number;
};

export function hueFor(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360;
  return h;
}

export function toAdminUserRow(e: AdminEntry): AdminUserRow {
  return {
    user: e.user,
    name: e.name,
    permissions: e.permissions,
    hue: hueFor(e.user),
  };
}

export type AdminUsersSource = "live" | "empty";

const ProfileSettingsListEnvelope = apiOkOf(z.array(z.unknown()));

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;
export type _DriftProfileSettingsListEnvelope = Assert<
  AssignableTo<RsApiOk<unknown[]>, z.input<typeof ProfileSettingsListEnvelope>>
>;
export type AdminUsersResult = {
  rows: AdminUserRow[];
  source: AdminUsersSource;
};

/** Shared row parser, kept for when the read path is actually fixed. */
export function parseAdminUsers(rows: unknown[]): AdminUserRow[] {
  const out: AdminEntry[] = [];
  for (const row of rows) {
    const r = AdminEntrySchema.safeParse(row);
    if (r.success) out.push(r.data);
  }
  return out.map(toAdminUserRow);
}

export const PROFILE_SETTINGS_PATH = "/events/api/profiles/settings";

/**
 * The What's-On moderator list -- reported as unavailable, not fetched.
 *
 * The server gate is real and wallet-satisfiable (no secret):
 *   catalyrst-events/src/handlers/profile_settings.rs:40-41
 *     require_auth(&headers, "get", "/api/profiles/settings")
 *     then authority::require_moderator
 *   catalyrst-fed/src/authority.rs:5-11  SELECT 1 FROM moderators WHERE address=$1
 *   catalyrst-fed/src/authority.rs:13-19 403 otherwise
 *
 * Three things are wrong on the client side and none of them is fixed here
 * (the build gate lists this as FIX-FIRST):
 *
 *   1. The request went to `/events/api/events/api/profiles/settings` -- the
 *      `/api` segment is doubled. Correct path is `PROFILE_SETTINGS_PATH`.
 *   2. No browser identity was passed, so the call 401s before it can 403;
 *      "not a moderator" was indistinguishable from "not connected".
 *   3. It must sign the UN-prefixed `/api/profiles/settings`. nginx
 *      `location /events/api/` (01-catalyst.conf:81) does not set
 *      `x-original-path` and `_proxy.inc` does not add it, so
 *      `signed_fetch_path` falls back to the route path -- the opposite
 *      convention from `/comms/`.
 *
 * The previous implementation also swallowed every failure into
 * `{ rows: [], source: "empty" }`, so a 403 rendered as an empty table. That
 * is the specific dishonesty this returns instead.
 *
 * There is deliberately no `hasAdminScope` argument any more: a caller-supplied
 * boolean is not authorization, and pretending otherwise is what this build
 * exists to remove.
 */
export function loadAdminUsers(
  _opts: GetOptions = {},
): ControlResult<AdminUserRow[]> {
  return controlStatus("whatson.users.read") as Unavailable;
}

export type ProfileSettingsPatchBody = {
  user: string;
  permissions: string[];
};

export type SaveResult = {
  user: string;
  permissions: string[];
  applied: boolean;
};

/**
 * Saving permissions -- BLOCK.
 *
 * The server gate is real and fails closed
 * (catalyrst-events/src/handlers/profile_settings.rs:145-155: federation
 * envelope preflight, then `require_moderator`). Nothing on this side reaches
 * it: the types above are declared and have no caller, and the signed-envelope
 * builder the endpoint requires does not exist in this repo. The control
 * renders explicitly unavailable rather than as a save button that cannot save.
 */
export function saveAdminUserPermissions(
  _body: ProfileSettingsPatchBody,
): Unavailable {
  return controlStatus("whatson.users.savePermissions") as Unavailable;
}

