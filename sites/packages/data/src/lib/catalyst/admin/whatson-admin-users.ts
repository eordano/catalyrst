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

export function parseAdminUsers(rows: unknown[]): AdminUserRow[] {
  const out: AdminEntry[] = [];
  for (const row of rows) {
    const r = AdminEntrySchema.safeParse(row);
    if (r.success) out.push(r.data);
  }
  return out.map(toAdminUserRow);
}

export const PROFILE_SETTINGS_PATH = "/events/api/profiles/settings";

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

export function saveAdminUserPermissions(
  _body: ProfileSettingsPatchBody,
): Unavailable {
  return controlStatus("whatson.users.savePermissions") as Unavailable;
}

