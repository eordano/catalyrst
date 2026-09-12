
import { z } from "zod";

const nullableStr = z.string().nullish();

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

export const EquippedSchema = z.object({
  bodyShape: nullableStr,
  skinColor: nullableStr,
  hairColor: nullableStr,
  eyeColor: nullableStr,
  name: nullableStr,
  wearables: z.array(z.string()).nullish(),
  emotes: z.array(z.string()).nullish(),
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
