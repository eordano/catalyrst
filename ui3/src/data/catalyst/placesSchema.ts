import { z } from "zod";

import { getJSON, type QueryParams, type RequestOpts } from "./client";
import { PLACES_LIMIT, toPlaceView, toCategoryView } from "./places";
import type { PlaceView, CategoryView } from "./places";

const nullableStr = z.string().nullish().transform((v) => v ?? null);
const nullableNum = z.number().nullish().transform((v) => v ?? null);

/**
 * Required here means required in `PlaceRow` (catalyrst-places), which serializes
 * every one of these unconditionally. A payload missing one is not a place with a
 * zero, it is not a place — dropping it beats rendering a parcel at the origin.
 */
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

export type Place = z.infer<typeof PlaceSchema>;

const ListEnvelope = z.object({
  ok: z.boolean(),
  data: z.array(z.unknown()),
  total: z.number(),
});

const ItemEnvelope = z.object({
  ok: z.boolean(),
  data: z.unknown().nullish(),
});

const CategorySchema = z.object({
  name: z.string(),
  active: z.boolean(),
  count: z.number(),
  i18n: z.object({ en: nullableStr }),
});

export type PlaceCategory = z.infer<typeof CategorySchema>;

const CategoriesEnvelope = z.object({
  ok: z.boolean(),
  data: z.array(z.unknown()),
});

export async function fetchPlaces(
  params: QueryParams = {},
  opts: RequestOpts = {},
): Promise<PlaceView[]> {
  const env = await getJSON("/api/places", {
    service: "places",
    ...opts,
    query: { limit: PLACES_LIMIT, ...params, ...(opts.query ?? {}) },
  });
  const parsed = ListEnvelope.safeParse(env);
  const rows = parsed.success ? parsed.data.data : [];
  const out: PlaceView[] = [];
  for (const raw of rows) {
    const r = PlaceSchema.safeParse(raw);
    if (r.success) out.push(toPlaceView(r.data));
  }
  return out;
}

export async function fetchPlace(
  id?: string | null,
  opts: RequestOpts = {},
): Promise<PlaceView | null> {
  if (!id) return null;
  const env = await getJSON(`/api/places/${encodeURIComponent(id)}`, {
    service: "places",
    ...opts,
  });
  const parsed = ItemEnvelope.safeParse(env);
  const raw = parsed.success ? parsed.data.data : null;
  const r = PlaceSchema.safeParse(raw);
  return r.success ? toPlaceView(r.data) : null;
}

export async function fetchWorlds(
  params: QueryParams = {},
  opts: RequestOpts = {},
): Promise<PlaceView[]> {
  const env = await getJSON("/api/worlds", {
    service: "places",
    ...opts,
    query: { limit: PLACES_LIMIT, ...params, ...(opts.query ?? {}) },
  });
  const parsed = ListEnvelope.safeParse(env);
  const rows = parsed.success ? parsed.data.data : [];
  const out: PlaceView[] = [];
  for (const raw of rows) {
    const r = PlaceSchema.safeParse(raw);
    if (r.success) out.push(toPlaceView(r.data));
  }
  return out;
}

export async function fetchCategories(opts: RequestOpts = {}): Promise<CategoryView[]> {
  const env = await getJSON("/api/categories", { service: "places", ...opts });
  const parsed = CategoriesEnvelope.safeParse(env);
  const rows = parsed.success ? parsed.data.data : [];
  const out: CategoryView[] = [];
  for (const raw of rows) {
    const r = CategorySchema.safeParse(raw);
    if (r.success && r.data.name) out.push(toCategoryView(r.data));
  }
  return out;
}
