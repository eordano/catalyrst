
import { z } from "zod";

const nullableStr = z.string().nullish();
const nullableNum = z.number().nullish();

export const PlaceSchema = z.object({
  id: z.string(),
  title: nullableStr,
  description: nullableStr,
  image: nullableStr,
  owner: nullableStr,
  creator_address: nullableStr,
  contact_name: nullableStr,
  base_position: z.string(),
  positions: z.array(z.string()),
  categories: z.array(z.string()),
  user_count: nullableNum,
  user_visits: z.number(),
  favorites: z.number(),
  likes: z.number(),
  like_rate: nullableNum,
  highlighted: z.boolean(),
  world: z.boolean(),
  world_name: nullableStr,
  updated_at: nullableStr,
});

export type PlaceWire = z.infer<typeof PlaceSchema>;

export const ListEnvelope = z.object({
  ok: z.boolean(),
  data: z.array(z.unknown()),
  total: z.number(),
});

export const ItemEnvelope = z.object({
  ok: z.boolean(),
  data: z.unknown().nullish(),
});

export const CategorySchema = z.object({
  name: z.string(),
  active: z.boolean(),
  count: z.number(),
  i18n: z.object({ en: nullableStr }),
});

export type PlaceCategoryWire = z.infer<typeof CategorySchema>;

export const CategoriesEnvelope = z.object({
  ok: z.boolean(),
  data: z.array(z.unknown()),
});
