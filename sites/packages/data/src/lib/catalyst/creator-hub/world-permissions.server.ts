import { z } from "zod";

import { getJSON, worldsBase, type GetOptions } from "../client";
import {
  PermissionsResponseSchema,
  WorldPermissionsBlockSchema,
} from "../generated-schemas/worlds";
import {
  ACCESS_TYPES,
  ACCESS_TYPES_CATALOG,
  WORLD_PERMISSION_LIMITS,
  WorldPermissionsSchema,
  emptyWorldPermissions,
  type AccessType,
  type Community,
  type WorldPermissions,
} from "./world-permissions";

export { worldsBase };

export type LoadWorldPermissionsResult = {
  permissions: WorldPermissions;
  source: "catalyst" | "empty";
  fallback: boolean;
};

/**
 * `type` is written unconditionally on all three legs by
 * `catalyrst-worlds/src/handlers/permissions.rs` ("allow-list" for deployment
 * and streaming, `access.to_public_json()` for access), so it is required.
 *
 * Defaulting it was the worst lie on this surface in both directions: a leg
 * that lost its `type` read back as `unrestricted`, which the panel draws as
 * "anyone can access this world" for a world whose ACL we simply failed to
 * read. The three legs are required for the same reason.
 */
const BackendAllowList = z.object({
  type: z.string(),
  wallets: z.array(z.string()).nullish().transform((v) => v ?? []),
});

const BackendAccess = z.object({
  type: z.string(),
  wallets: z.array(z.string()).nullish().transform((v) => v ?? []),
  communities: z.array(z.string()).nullish().transform((v) => v ?? []),
});

// Composed on the generated wire schemas (the zod-dedup rule); the extends
// reinstate the tolerant legs the wire type leaves loose -- access is typed
// here (generated leaves it unknown) and absent lists parse to [] rather
// than failing a world whose ACL we merely failed to read.
const BackendPermissionsSchema = PermissionsResponseSchema.extend({
  permissions: WorldPermissionsBlockSchema.extend({
    access: BackendAccess,
    deployment: BackendAllowList,
    streaming: BackendAllowList,
  }),
  owner: z.string().nullish().transform((v) => v ?? null),
  summary: z
    .record(
      z.string(),
      z.array(
        z.object({
          permission: z.string(),
          world_wide: z.boolean().nullish(),
        }),
      ),
    )
    .nullish(),
});

type BackendPermissions = z.infer<typeof BackendPermissionsSchema>;

function normalizeAccessType(raw: string | undefined): AccessType {
  if (raw && (ACCESS_TYPES as readonly string[]).includes(raw)) {
    return raw as AccessType;
  }
  return "allow-list";
}

const CommunityRefSchema = z.object({
  id: z.string(),
  name: z.string().nullish().transform((v) => v ?? null),
  membersCount: z.number().nullish().transform((v) => v ?? null),
});

function unwrapData<T = unknown>(env: unknown): T {
  const e = env as { data?: T } | null;
  return (e?.data ?? env) as T;
}

async function resolveCommunities(
  ids: string[],
  opts: GetOptions,
): Promise<Community[]> {
  if (ids.length === 0) return [];
  const resolved = await Promise.all(
    ids.map(async (id) => {
      try {
        const raw = await getJSON<unknown>(
          `/v1/communities/${encodeURIComponent(id)}`,
          { signal: opts.signal, fetchImpl: opts.fetchImpl },
        );
        const parsed = CommunityRefSchema.safeParse(unwrapData(raw));
        return parsed.success ? parsed.data : null;
      } catch {
        return null;
      }
    }),
  );
  return resolved.filter((c): c is Community => c !== null);
}

function adaptBackendPermissions(
  worldName: string,
  raw: BackendPermissions,
  communities: Community[],
): WorldPermissions {
  const owner = raw.owner;

  const collaborators = Object.entries(raw.summary ?? {}).map(
    ([address, perms]) => ({
      address,
      name: null,
      role: "collaborator",
      deployment: perms.some((p) => p.permission === "deployment")
        ? ("world-wide" as const)
        : ("none" as const),
      streaming: perms.some((p) => p.permission === "streaming"),
      parcels: 0,
    }),
  );

  return {
    world: { name: worldName, owner },
    permissions: {
      access: {
        type: normalizeAccessType(raw.permissions.access.type),
        wallets: raw.permissions.access.wallets,
        communities: raw.permissions.access.communities,
      },
      deployment: {
        type: raw.permissions.deployment.type,
        wallets: raw.permissions.deployment.wallets,
      },
      streaming: {
        type: raw.permissions.streaming.type,
        wallets: raw.permissions.streaming.wallets,
      },
    },
    owner,
    communities,
    collaborators,
    accessTypes: ACCESS_TYPES_CATALOG,
    limits: WORLD_PERMISSION_LIMITS,
  };
}

export async function loadWorldPermissions(
  worldName: string,
  opts: GetOptions = {},
): Promise<LoadWorldPermissionsResult> {
  try {
    const raw = await getJSON<unknown>(
      `/world/${encodeURIComponent(worldName)}/permissions`,
      { ...opts, base: opts.base ?? worldsBase() },
    );
    const backend = BackendPermissionsSchema.safeParse(raw);
    if (backend.success) {
      const communities = await resolveCommunities(
        backend.data.permissions.access.communities,
        opts,
      );
      const adapted = adaptBackendPermissions(worldName, backend.data, communities);
      const view = WorldPermissionsSchema.safeParse(adapted);
      if (view.success)
        return { permissions: view.data, source: "catalyst", fallback: false };
    }
  } catch {
  }
  return {
    permissions: emptyWorldPermissions(worldName),
    source: "empty",
    fallback: true,
  };
}
