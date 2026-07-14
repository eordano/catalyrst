import { getJSON, type QueryParams, type RequestOpts } from "./client";
import {
  PLACES_LIMIT,
  isRenderablePlace,
  isRenderablePlaceCategory,
  normalizePlace,
  normalizePlaceCategory,
  toPlaceView,
  toCategoryView,
} from "./places";
import type { PlaceView, CategoryView } from "./places";
import { field, keepRow, keepRows } from "./rows";
import {
  CategoriesEnvelope,
  CategorySchema,
  ItemEnvelope,
  ListEnvelope,
  PlaceSchema,
} from "./schemas/places";

export { PlaceSchema };
export type { Place, PlaceCategory } from "./places";

// Every read here is schema-then-guard (see rows.ts): the schema says whether
// the row is correct and leaves with the perf build, the guard says whether
// `toPlaceView` can use it and stays in both.
//
// The envelope is read through `field` for the same reason. `ListEnvelope`
// proves `data` is an array only in the checking build; in perf the stub says
// yes to a 404 body, and `for (const row of undefined)` throws before a single
// row is looked at.

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
  return keepRows(
    parsed.success ? field(parsed.data, "data") : [],
    PlaceSchema,
    isRenderablePlace,
    (row) => toPlaceView(normalizePlace(row)),
  );
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
  const row = keepRow(
    parsed.success ? field(parsed.data, "data") : null,
    PlaceSchema,
    isRenderablePlace,
  );
  return row ? toPlaceView(normalizePlace(row)) : null;
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
  return keepRows(
    parsed.success ? field(parsed.data, "data") : [],
    PlaceSchema,
    isRenderablePlace,
    (row) => toPlaceView(normalizePlace(row)),
  );
}

export async function fetchCategories(opts: RequestOpts = {}): Promise<CategoryView[]> {
  const env = await getJSON("/api/categories", { service: "places", ...opts });
  const parsed = CategoriesEnvelope.safeParse(env);
  return keepRows(
    parsed.success ? field(parsed.data, "data") : [],
    CategorySchema,
    isRenderablePlaceCategory,
    (row) => toCategoryView(normalizePlaceCategory(row)),
  );
}
