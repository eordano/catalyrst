import { z } from "zod";

import type { GetOptions } from "../client";
import { apiOkOf } from "../envelope";
import { controlStatus } from "./control-availability";
import type { ControlResult, Unavailable } from "./availability";
import type { ApiOk as RsApiOk } from "@ui/generated/catalyst/events/ApiOk";

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;

type Assert<T extends true> = T;

export type _DriftProfileSettingsListEnvelope = Assert<
  AssignableTo<RsApiOk<unknown[]>, z.input<typeof ProfileSettingsListEnvelope>>
>;

type AdminUserRow = {
  user: string;
  name: string | null;
  permissions: string[];
  hue: number;
};

const ProfileSettingsListEnvelope = apiOkOf(z.array(z.unknown()));

export function loadAdminUsers(
  _opts: GetOptions = {},
): ControlResult<AdminUserRow[]> {
  return controlStatus("whatson.users.read") as Unavailable;
}

