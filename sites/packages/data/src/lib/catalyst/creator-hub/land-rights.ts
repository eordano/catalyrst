import { z } from "zod";

import { CatalystError, getJSON } from "../client";
import type { GetOptions } from "../client";
import { DEPLOY_GRANTING_LEGS } from "@ui/generated/catalyst/validator/deployGrantingLegs";
import type { ParcelPermissionFlags } from "@ui/generated/catalyst/validator/ParcelPermissionFlags";

export type { ParcelPermissionFlags };
export { DEPLOY_GRANTING_LEGS };

export type LandRights =
  | { status: "granted"; parcels: string[] }
  | { status: "denied"; parcel: string; owner: string | null }
  | { status: "unknown"; parcel: string };

const PROBE_BATCH = 6;

/**
 * Every leg is required. A payload that omits one is not a permission answer,
 * and defaulting it to `false` would deny a deploy the wallet may actually be
 * entitled to; a failed parse becomes `status: "unknown"` instead.
 */
const PermissionFlagsSchema = z.object({
  owner: z.boolean(),
  operator: z.boolean(),
  updateOperator: z.boolean(),
  updateManager: z.boolean(),
  approvedForAll: z.boolean(),
});

const NO_RIGHTS: ParcelPermissionFlags = {
  owner: false,
  operator: false,
  updateOperator: false,
  updateManager: false,
  approvedForAll: false,
};

const OperatorsSchema = z.object({
  owner: z.string().nullish().transform((v) => v ?? null),
});

export function parseParcel(pointer: string): { x: number; y: number } | null {
  const m = /^\s*(-?\d+)\s*,\s*(-?\d+)\s*$/.exec(pointer);
  if (!m) return null;
  return { x: Number.parseInt(m[1], 10), y: Number.parseInt(m[2], 10) };
}

export function grantsDeploy(flags: ParcelPermissionFlags): boolean {
  return DEPLOY_GRANTING_LEGS.some((leg) => flags[leg]);
}

export async function fetchParcelPermissions(
  address: string,
  pointer: string,
  opts: GetOptions = {},
): Promise<ParcelPermissionFlags | null> {
  const coord = parseParcel(pointer);
  if (!coord) return null;
  const addr = address.trim().toLowerCase();
  try {
    const raw = await getJSON<unknown>(
      `/lambdas/users/${encodeURIComponent(addr)}/parcels/${coord.x}/${coord.y}/permissions`,
      opts,
    );
    const parsed = PermissionFlagsSchema.safeParse(raw);
    return parsed.success ? parsed.data : null;
  } catch (err) {
    if (err instanceof CatalystError && err.status === 404) {
      return NO_RIGHTS;
    }
    throw err;
  }
}

export async function fetchParcelOwner(
  pointer: string,
  opts: GetOptions = {},
): Promise<string | null> {
  const coord = parseParcel(pointer);
  if (!coord) return null;
  try {
    const raw = await getJSON<unknown>(
      `/lambdas/parcels/${coord.x}/${coord.y}/operators`,
      opts,
    );
    const parsed = OperatorsSchema.safeParse(raw);
    return parsed.success ? parsed.data.owner : null;
  } catch {
    return null;
  }
}

export async function probeLandRights(
  address: string,
  pointers: readonly string[],
  opts: GetOptions = {},
): Promise<LandRights> {
  const addr = address.trim().toLowerCase();
  const parcels = Array.from(new Set(pointers.map((p) => p.trim()).filter(Boolean)));
  if (!addr || parcels.length === 0) {
    return { status: "unknown", parcel: parcels[0] ?? "" };
  }
  for (const parcel of parcels) {
    if (!parseParcel(parcel)) return { status: "unknown", parcel };
  }

  for (let i = 0; i < parcels.length; i += PROBE_BATCH) {
    const batch = parcels.slice(i, i + PROBE_BATCH);
    const results = await Promise.all(
      batch.map(async (parcel) => {
        try {
          const flags = await fetchParcelPermissions(addr, parcel, opts);
          if (!flags) return { parcel, outcome: "unknown" as const };
          return grantsDeploy(flags)
            ? { parcel, outcome: "granted" as const }
            : { parcel, outcome: "denied" as const };
        } catch {
          return { parcel, outcome: "unknown" as const };
        }
      }),
    );
    for (const r of results) {
      if (r.outcome === "denied") {
        const owner = await fetchParcelOwner(r.parcel, opts);
        return { status: "denied", parcel: r.parcel, owner };
      }
      if (r.outcome === "unknown") {
        return { status: "unknown", parcel: r.parcel };
      }
    }
  }
  return { status: "granted", parcels };
}
