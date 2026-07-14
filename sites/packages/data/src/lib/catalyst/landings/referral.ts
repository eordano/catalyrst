import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { shortAddress, ETH_ADDRESS_RE } from "../format/address";
import { NameColorSchema } from "../generated-schemas/communities";

/** The generated {r,g,b} triple, softened: lambdas profiles may omit channels. */
const Color3Schema = NameColorSchema.partial().optional();

const AvatarSchema = z.object({
  userId: z.string().optional(),
  name: z.string().optional(),
  ethAddress: z.string().optional(),
  hasClaimedName: z.boolean().optional(),
  nameColor: z.union([z.string(), Color3Schema]).optional(),
});

const ProfileLambdaSchema = z.object({
  avatars: z.array(AvatarSchema),
});
export type ProfileLambda = z.infer<typeof ProfileLambdaSchema>;

export const ReferralTierSchema = z.object({
  tier: z.number(),
  invitesAccepted: z.number(),
  rarity: z.string(),
  description: z.string(),
});

export const ReferralStateSchema = z.object({
  invitedUsers: z.number(),
  invitedUsersAccepted: z.number(),
  currentTier: z.number(),
  nextTier: z.number(),
  tiers: z.array(ReferralTierSchema),
});
export type ReferralState = z.infer<typeof ReferralStateSchema>;

export type ReferrerVM = {
  name: string;
  address: string | null;
  hasClaimedName: boolean;
  resolved: boolean;
};

export function truncateAddress(addr: string): string {
  return shortAddress(addr);
}

export function toReferrerVM(profile: ProfileLambda): ReferrerVM | null {
  const a = profile.avatars[0];
  if (!a) return null;
  const claimed = Boolean(a.hasClaimedName && a.name);
  const address = a.ethAddress ? a.ethAddress.toLowerCase() : null;
  const name = claimed
    ? (a.name as string)
    : address
      ? truncateAddress(address)
      : "A friend";
  return { name, address, hasClaimedName: claimed, resolved: true };
}

export function referrerFromParam(param: string): ReferrerVM {
  const p = param.trim();
  if (ETH_ADDRESS_RE.test(p)) {
    return { name: truncateAddress(p.toLowerCase()), address: p.toLowerCase(), hasClaimedName: false, resolved: false };
  }
  return { name: p || "A friend", address: null, hasClaimedName: false, resolved: false };
}

export async function fetchReferrerProfile(
  address: string,
  opts: GetOptions = {},
): Promise<ReferrerVM | null> {
  const raw = await getJSON<unknown>(
    `/lambdas/profiles/${encodeURIComponent(address.toLowerCase())}`,
    opts,
  );
  const profile = ProfileLambdaSchema.parse(raw);
  return toReferrerVM(profile);
}
