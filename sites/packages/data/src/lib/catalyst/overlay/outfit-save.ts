import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { z } from "zod";
import { NameColorSchema } from "../generated-schemas/communities";

export const FREE_SLOTS = 5;
export const TOTAL_SLOTS = 10;

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

/**
 * `Color3` requires all three channels, and an `Outfits` entry requires
 * `bodyShape`, `eyes`, `hair`, `skin` and `wearables` (`@dcl/schemas`
 * `platform/outfits`). None of them get a fallback here.
 *
 * `wearables: []` is the destructive one: an outfit slot that lists no items
 * is a *naked* outfit, and the wizard offers to apply it. A slot whose
 * wearables could not be read is dropped instead, so nothing offers to undress
 * an avatar on the strength of a truncated read.
 */
const Color3Schema = NameColorSchema.passthrough();

export const OutfitSchema = z
  .object({
    bodyShape: z.string(),
    eyes: z.object({ color: Color3Schema }).passthrough().optional(),
    hair: z.object({ color: Color3Schema }).passthrough().optional(),
    skin: z.object({ color: Color3Schema }).passthrough().optional(),
    wearables: z.array(z.string()),
    forceRender: z.array(z.string()).optional(),
  })
  .passthrough();
export type Outfit = z.infer<typeof OutfitSchema>;

export const OutfitSlotSchema = z.object({
  slot: z.number().int().nonnegative(),
  outfit: OutfitSchema,
});
export type OutfitSlot = z.infer<typeof OutfitSlotSchema>;

/** `outfits` and `namesForExtraSlots` are both required on the entity. The
 *  names list is what unlocks slots 6-10, so defaulting it to `[]` answered
 *  "you own no NAME" for a read that never happened, and the wizard refused a
 *  save the wallet was entitled to. */
export const OutfitsSchema = z
  .object({
    outfits: z.array(OutfitSlotSchema),
    namesForExtraSlots: z.array(z.string()),
  })
  .passthrough();
export type Outfits = z.infer<typeof OutfitsSchema>;

const AvatarInfoSchema = z
  .object({
    bodyShape: z.string().optional(),
    eyes: z.object({ color: Color3Schema }).passthrough().optional(),
    hair: z.object({ color: Color3Schema }).passthrough().optional(),
    skin: z.object({ color: Color3Schema }).passthrough().optional(),
    wearables: z.array(z.string()),
    forceRender: z.array(z.string()).optional(),
  })
  .passthrough();

/** `name`, `hasClaimedName` and `avatars` are all required on a profile
 *  (`@dcl/schemas` `platform/profile`); `hasClaimedName` in particular decides
 *  whether the extra outfit slots unlock at all. */
const AvatarSchema = z
  .object({
    name: z.string(),
    hasClaimedName: z.boolean(),
    avatar: AvatarInfoSchema.optional(),
  })
  .passthrough();

const ProfileSchema = z.object({ avatars: z.array(AvatarSchema) }).passthrough();

export const ProfilesBatchSchema = z.array(ProfileSchema);

export type EquippedSet = Outfit;

export type OutfitSaveData = {
  address: string;
  profileEmpty: boolean;
  freeSlots: number;
  totalSlots: number;
  equipped: EquippedSet;
  namesForExtraSlots: string[];
  outfits: OutfitSlot[];
  source: "live" | "fixture";
};

export function canSaveToSlot(args: {
  slot: number;
  name: string;
  freeSlots: number;
  totalSlots: number;
  namesForExtraSlots: string[];
}): { ok: boolean; reason?: "out-of-range" | "needs-name" | "no-name-unlock" } {
  const { slot, name, freeSlots, totalSlots, namesForExtraSlots } = args;
  if (slot < 0 || slot >= totalSlots) return { ok: false, reason: "out-of-range" };
  const isExtra = slot >= freeSlots;
  if (!isExtra) return { ok: true };
  if (namesForExtraSlots.length === 0) return { ok: false, reason: "no-name-unlock" };
  if (!name.trim()) return { ok: false, reason: "needs-name" };
  return { ok: true };
}

export function isExtraSlot(slot: number, freeSlots = FREE_SLOTS): boolean {
  return slot >= freeSlots;
}

export async function fetchOutfitSaveData(
  address: string,
  opts: GetOptions = {},
): Promise<OutfitSaveData | null> {
  const addr = normalizeAddress(address);
  const { catalystBase } = await import("../client");
  const base = catalystBase(opts.base);
  let raw: unknown;
  try {
    const res = await (opts.fetchImpl ?? fetch)(`${base}/lambdas/profiles`, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({ ids: [addr] }),
      signal: opts.signal,
    });
    if (!res.ok) return null;
    raw = await res.json();
  } catch {
    return null;
  }

  const parsed = ProfilesBatchSchema.safeParse(raw);
  if (!parsed.success) return null;
  const avatar = parsed.data[0]?.avatars[0];
  if (!avatar || !avatar.avatar) return null;

  const info = avatar.avatar;
  const equippedParsed = OutfitSchema.safeParse({
    bodyShape: info.bodyShape,
    eyes: info.eyes,
    hair: info.hair,
    skin: info.skin,
    wearables: info.wearables,
    forceRender: info.forceRender,
  });
  if (!equippedParsed.success) return null;
  const equipped: EquippedSet = equippedParsed.data;

  const rawOutfits = (avatar as Record<string, unknown>).outfits;
  const outfitsParsed = OutfitsSchema.safeParse(rawOutfits);
  const outfits = outfitsParsed.success ? outfitsParsed.data.outfits : [];
  const namesForExtraSlots = outfitsParsed.success
    ? outfitsParsed.data.namesForExtraSlots
    : avatar.hasClaimedName && avatar.name
      ? [avatar.name]
      : [];

  return {
    address: addr,
    profileEmpty: false,
    freeSlots: FREE_SLOTS,
    totalSlots: TOTAL_SLOTS,
    equipped,
    namesForExtraSlots,
    outfits,
    source: "live",
  };
}
