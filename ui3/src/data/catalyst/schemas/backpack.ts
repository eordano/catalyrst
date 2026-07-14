// Wire shapes for the backpack reader -- wearables, emotes and what a profile
// says is equipped.
//
// Schemas and the WIRE types they infer, and nothing else. A perf build aliases
// this whole module to a generated stub (vite.validate.js), which is what lets
// zod leave the bundle -- so a transform put here would run in one build and not
// the other, and the stub would be changing behaviour rather than only changing
// what is checked.
//
// The nullish -> null normalization the exported `Wearable`, `SlotBinding`,
// `Equipped` and `Emote` types promise, and the emote-category bucketing, live
// in ../backpack.ts next to the readers they feed, and run in both modes.

import { z } from "zod";

const nullableStr = z.string().nullish();

/**
 * Every field is filled in by `mapExplorerWearable` from the entity itself, so a
 * candidate that cannot supply one describes an item nothing can render. A
 * defaulted category silently filed such an item under `upper_body`.
 */
export const WearableSchema = z.object({
  urn: z.string().min(1),
  name: z.string(),
  thumbnail: nullableStr,
  rarity: z.string(),
  category: z.string(),
  bodyShapes: z.array(z.string()),
  description: nullableStr,
  isSmart: z.boolean(),
  creator: nullableStr,
  network: nullableStr,
});

export type WearableWire = z.infer<typeof WearableSchema>;

export const CategorySchema = z.object({
  id: z.string(),
  label: z.string(),
  slot: z.string(),
});

export type WearableCategory = z.infer<typeof CategorySchema>;

export const SlotBindingSchema = z.object({
  slot: z.number().int().min(0).max(9),
  urn: z.string().min(1),
  name: nullableStr,
});

export type SlotBindingWire = z.infer<typeof SlotBindingSchema>;

/**
 * What the profile actually says, with null for "the profile does not say".
 * The look the renderer falls back to is a display decision and belongs to the
 * render sites, which already name their own DEFAULT_BODY / DEFAULT_SKIN -- a
 * default here made an unreadable profile indistinguishable from a BaseMale.
 */
export const EquippedSchema = z.object({
  bodyShape: nullableStr,
  skinColor: nullableStr,
  hairColor: nullableStr,
  eyeColor: nullableStr,
  name: nullableStr,
  wearables: z.array(z.string()).nullish(),
  emotes: z.array(z.string()).nullish(),
  // Real per-slot emote-wheel assignment from the profile (avatar.emotes:
  // [{slot,urn}]) -- distinct from `emotes` above, which is just the flat
  // urn list used for the SetAvatar equip payload.
  emoteSlots: z.array(SlotBindingSchema).nullish(),
});

export type EquippedWire = z.infer<typeof EquippedSchema>;

export const OwnedElementSchema = z.object({ urn: z.string() }).passthrough();

export const EmoteSchema = z.object({
  urn: z.string().min(1),
  name: z.string(),
  description: nullableStr,
  thumbnail: nullableStr,
  rarity: nullableStr,
  category: nullableStr,
  loop: z.boolean().nullish(),
});

export type EmoteWire = z.infer<typeof EmoteSchema>;

export const OwnedEmoteElementSchema = z.object({ urn: z.string() }).passthrough();
