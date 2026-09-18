import type { GetOptions } from "../client";
import { z } from "zod";
import { NameColorSchema } from "../generated-schemas/communities";

const FREE_SLOTS = 5;
const TOTAL_SLOTS = 10;

function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

const Color3Schema = NameColorSchema.passthrough();

const OutfitSchema = z
  .object({
    bodyShape: z.string(),
    eyes: z.object({ color: Color3Schema }).passthrough().optional(),
    hair: z.object({ color: Color3Schema }).passthrough().optional(),
    skin: z.object({ color: Color3Schema }).passthrough().optional(),
    wearables: z.array(z.string()),
    forceRender: z.array(z.string()).optional(),
  })
  .passthrough();
type Outfit = z.infer<typeof OutfitSchema>;

const OutfitSlotSchema = z.object({
  slot: z.number().int().nonnegative(),
  outfit: OutfitSchema,
});
type OutfitSlot = z.infer<typeof OutfitSlotSchema>;

const OutfitsSchema = z
  .object({
    outfits: z.array(OutfitSlotSchema),
    namesForExtraSlots: z.array(z.string()),
  })
  .passthrough();

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

const AvatarSchema = z
  .object({
    name: z.string(),
    hasClaimedName: z.boolean(),
    avatar: AvatarInfoSchema.optional(),
  })
  .passthrough();

const ProfileSchema = z.object({ avatars: z.array(AvatarSchema) }).passthrough();

const ProfilesBatchSchema = z.array(ProfileSchema);

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
