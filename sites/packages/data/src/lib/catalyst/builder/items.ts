import { z } from "zod";
import { warnInvalid } from "../warn";

export const ITEM_TYPES = ["wearable", "emote"] as const;

export const RARITIES = [
  "unique",
  "mythic",
  "exotic",
  "legendary",
  "epic",
  "rare",
  "uncommon",
  "common",
] as const;
export type Rarity = (typeof RARITIES)[number];

export const BODY_SHAPES = ["male", "female", "both"] as const;

const WearableMetricsSchema = z.object({
  triangles: z.number(),
  materials: z.number(),
  textures: z.number(),
});

const EmoteMetricsSchema = z.object({
  sequences: z.number(),
  duration: z.number(),
  frames: z.number(),
  fps: z.number(),
});

const RepresentationSchema = z.object({
  mainFile: z.string(),
  bodyShape: z.enum(BODY_SHAPES),
});

const nullableStr = z.string().nullish().transform((v) => v ?? null);

/**
 * Every field is required on purpose.
 *
 * The only producer is `creator-hub/wearable-item-detail.server.ts`
 * (`catalogItemToBuilderItem`), which builds the object key by key from a
 * parsed `CatalogItem` and supplies all of them. Defaulting anything here would
 * mean `safeParse` could not reject a half-built item -- a supply of 0 or an
 * empty representation list would render as a measured fact. A missing field is
 * a bug in the producer, so it must fail: `parseItem` returns null and the
 * route renders its not-found state. The enums carry no `.catch()` for the
 * same reason: a rarity nobody recognised is not a common item, and
 * `RARITY_MAX_SUPPLY` would still have priced it off the unrecognised name.
 */
export const BuilderItemSchema = z.object({
  id: z.string(),
  type: z.enum(ITEM_TYPES),
  name: z.string(),
  description: z.string(),
  utility: z.string(),
  rarity: z.enum(RARITIES),
  category: z.string(),
  bodyShape: z.enum(BODY_SHAPES),
  smart: z.boolean(),
  isPublished: z.boolean(),
  tokenId: nullableStr,
  totalSupply: z.number(),
  maxSupply: z.number(),
  collection: z.string(),
  urn: z.string(),
  price: z.string(),
  beneficiary: z.string(),
  hue: z.number(),
  tags: z.array(z.string()),
  metrics: z.union([WearableMetricsSchema, EmoteMetricsSchema]),
  representations: z.array(RepresentationSchema),
  requiredPermissions: z.array(z.string()),
});
export type BuilderItem = z.infer<typeof BuilderItemSchema>;

export function parseItem(raw: unknown): BuilderItem | null {
  const r = BuilderItemSchema.safeParse(raw);
  if (r.success) return r.data;
  warnInvalid("BuilderItem", r.error.issues);
  return null;
}

