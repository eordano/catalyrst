import { z } from "zod";
import { postJSON } from "../client";
import type { AuthIdentity } from "../../auth/types";
import { shortAddress, ETH_ADDRESS_RE } from "../format/address";
import { warnInvalid } from "../warn";

export const SceneAdminRowSchema = z.object({
  id: z.string().nullish().transform((v) => v ?? null),
  place_id: z.string().nullish().transform((v) => v ?? null),
  added_by: z.string().nullish().transform((v) => v ?? null),
  created_at: z.number().nullish().transform((v) => v ?? null),
  active: z.boolean().nullish().transform((v) => v ?? null),
  admin: z.string(),
  name: z.string().nullish().transform((v) => v ?? null),
  canBeRemoved: z.boolean(),
});
export type SceneAdminRow = z.infer<typeof SceneAdminRowSchema>;

/**
 * `GET /places/api/places` rows -- `catalyrst-places/src/ports/places/rows.rs:43-95`.
 *
 * `positions`, `base_position` and `world` are non-optional there, so they are
 * required here. `base_position` in particular was defaulting to "0,0", which
 * is a real parcel: a place whose coordinates failed to arrive was rendered,
 * and linked, as the place at the origin. Such a row is dropped by
 * `parsePlaces` now.
 */
export const OperatedPlaceSchema = z.object({
  id: z.string(),
  title: z.string().nullish().transform((v) => v ?? null),
  base_position: z.string(),
  positions: z.array(z.string()),
  world: z.boolean(),
  world_name: z.string().nullish().transform((v) => v ?? null),
  owner: z.string().nullish().transform((v) => v ?? null),
  image: z.string().nullish().transform((v) => v ?? null),
});
export type OperatedPlace = z.infer<typeof OperatedPlaceSchema>;

export const FixtureSchema = z.object({
  _source: z.string().optional(),
  owner: z.string(),
  places: z.array(OperatedPlaceSchema),
  grants: z.record(z.string(), z.array(SceneAdminRowSchema)),
});

export type GrantKind = "explicit" | "implicit";

export type AdminEntry = {
  admin: string;
  /** Null when the grant carries no profile name; render the address instead. */
  name: string | null;
  kind: GrantKind;
  canBeRemoved: boolean;
  addedBy: string | null;
  createdAt: number | null;
};

export function rowKind(row: SceneAdminRow): GrantKind {
  return row.id ? "explicit" : "implicit";
}

export function toAdminEntry(row: SceneAdminRow): AdminEntry {
  return {
    admin: row.admin.toLowerCase(),
    name: row.name,
    kind: rowKind(row),
    canBeRemoved: row.canBeRemoved,
    addedBy: row.added_by,
    createdAt: row.created_at,
  };
}

export function partitionGrants(rows: SceneAdminRow[]): {
  explicit: AdminEntry[];
  implicit: AdminEntry[];
} {
  const explicit: AdminEntry[] = [];
  const implicit: AdminEntry[] = [];
  for (const row of rows) {
    const entry = toAdminEntry(row);
    (entry.kind === "explicit" ? explicit : implicit).push(entry);
  }
  return { explicit, implicit };
}

export function parsePlaces(raw: unknown): OperatedPlace[] {
  if (!Array.isArray(raw)) {
    warnInvalid("OperatedPlace list", "not an array");
    return [];
  }
  const out: OperatedPlace[] = [];
  for (const item of raw) {
    const r = OperatedPlaceSchema.safeParse(item);
    if (r.success) out.push(r.data);
    else warnInvalid("OperatedPlace", r.error.issues);
  }
  return out;
}

export function isValidAddress(addr: string | null | undefined): boolean {
  return ETH_ADDRESS_RE.test((addr ?? "").trim());
}

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

export function truncateAddress(addr: string): string {
  return shortAddress(addr);
}

export type AdminAction = "add" | "revoke";

const SCENE_ADMIN_METADATA = { signer: "decentraland-kernel-scene" } as const;

export async function commitSceneAdmin(args: {
  identity: AuthIdentity;
  placeId: string;
  action: AdminAction;
  admin: string;
  base?: string;
  signal?: AbortSignal;
}): Promise<{ ok: true }> {
  const { identity, placeId, action, admin, base, signal } = args;
  const target = normalizeAddress(admin);
  if (action === "add") {
    await postJSON<void>(
      "/scene-admin",
      { place_id: placeId, admin: target },
      { identity, method: "POST", base, metadata: SCENE_ADMIN_METADATA, signal },
    );
  } else {
    await postJSON<void>(
      "/scene-admin",
      undefined,
      {
        identity,
        method: "DELETE",
        base,
        query: { place_id: placeId, admin: target },
        metadata: SCENE_ADMIN_METADATA,
        signal,
      },
    );
  }
  return { ok: true };
}

