import { z } from "zod";

import { ETH_ADDRESS_RE } from "../format/address";
import {
  AllowListPermissionSchema,
  WorldPermissionsBlockSchema,
} from "../generated-schemas/worlds";

export const ACCESS_TYPES = ["unrestricted", "allow-list", "shared-secret"] as const;
export type AccessType = (typeof ACCESS_TYPES)[number];

export function isValidAddress(addr: string): boolean {
  return ETH_ADDRESS_RE.test(addr.trim());
}

/**
 * This is the ACL panel's own shape, not a wire shape: the only thing that
 * reaches it is `adaptBackendPermissions`, which builds every field from a
 * `BackendPermissionsSchema` parse, plus `emptyWorldPermissions` below.
 *
 * Every field is therefore required. A default on any of them was a permission
 * invented for a producer that had stopped supplying it -- an empty `wallets`
 * reads as "nobody is on the allow list", `deployment: "none"` and
 * `streaming: false` strip a collaborator of rights they hold, and defaulted
 * `limits` would let the form accept a password or a wallet count the server
 * then rejects. With them required, a drifted adapter fails this parse and
 * `loadWorldPermissions` answers `fallback: true`, which the route renders as
 * "we could not read this world's permissions" rather than as an ACL.
 */
const AccessSettingSchema = z.object({
  type: z.enum(ACCESS_TYPES),
  wallets: z.array(z.string()),
  communities: z.array(z.string()),
});

const CommunitySchema = z.object({
  id: z.string(),
  name: z.string().nullable(),
  membersCount: z.number().nullable(),
});

const CollaboratorSchema = z.object({
  address: z.string(),
  name: z.string().nullable(),
  role: z.string(),
  deployment: z.enum(["world-wide", "parcels", "none"]),
  streaming: z.boolean(),
  parcels: z.number(),
});

const AccessTypeOptionSchema = z.object({
  value: z.enum(ACCESS_TYPES),
  label: z.string(),
  description: z.string(),
  requiresPassword: z.boolean(),
});

const LimitsSchema = z.object({
  maxCollaborators: z.number(),
  maxWallets: z.number(),
  maxCommunities: z.number(),
  minPasswordLength: z.number(),
  minPasswordNumbers: z.number(),
});

export const WorldPermissionsSchema = z.object({
  world: z.object({ name: z.string(), owner: z.string().nullable() }),
  // The generated wire block, with access narrowed to this page's setting
  // shape (the wire leaves it unknown).
  permissions: WorldPermissionsBlockSchema.extend({
    access: AccessSettingSchema,
  }),
  owner: z.string().nullable(),
  communities: z.array(CommunitySchema),
  collaborators: z.array(CollaboratorSchema),
  accessTypes: z.array(AccessTypeOptionSchema),
  limits: LimitsSchema,
});

export type WorldPermissions = z.infer<typeof WorldPermissionsSchema>;
export type Collaborator = z.infer<typeof CollaboratorSchema>;
export type Community = z.infer<typeof CommunitySchema>;
export type AccessTypeOption = z.infer<typeof AccessTypeOptionSchema>;
export type Limits = z.infer<typeof LimitsSchema>;

export const ACCESS_TYPES_CATALOG: AccessTypeOption[] = [
  {
    value: "unrestricted",
    label: "Public",
    description: "Anyone can access this world",
    requiresPassword: false,
  },
  {
    value: "allow-list",
    label: "Invitation only",
    description:
      "Only addresses and communities included in the whitelist can join.",
    requiresPassword: false,
  },
  {
    value: "shared-secret",
    label: "Password protected",
    description: "Only users who know the access password can join",
    requiresPassword: true,
  },
];

export const WORLD_PERMISSION_LIMITS: Limits = {
  maxCollaborators: 100,
  maxWallets: 1000,
  maxCommunities: 50,
  minPasswordLength: 8,
  minPasswordNumbers: 2,
};

export function emptyWorldPermissions(worldName = ""): WorldPermissions {
  return {
    world: { name: worldName, owner: null },
    permissions: {
      access: { type: "unrestricted", wallets: [], communities: [] },
      deployment: { type: "allow-list", wallets: [] },
      streaming: { type: "allow-list", wallets: [] },
    },
    owner: null,
    communities: [],
    collaborators: [],
    accessTypes: ACCESS_TYPES_CATALOG,
    limits: WORLD_PERMISSION_LIMITS,
  };
}

export function worldExists(p: WorldPermissions, sceneCount = 0): boolean {
  return (p.owner ?? "").trim() !== "" || sceneCount > 0;
}

export type WorldViewerRole = "owner" | "collaborator" | "none";

export function viewerWorldRole(
  p: WorldPermissions,
  viewer: string,
): WorldViewerRole {
  const v = viewer.trim().toLowerCase();
  if (!v) return "none";
  if ((p.owner ?? "").trim().toLowerCase() === v) return "owner";
  const listed =
    p.collaborators.some((c) => c.address.trim().toLowerCase() === v) ||
    p.permissions.deployment.wallets.some((w) => w.trim().toLowerCase() === v) ||
    p.permissions.streaming.wallets.some((w) => w.trim().toLowerCase() === v);
  return listed ? "collaborator" : "none";
}
