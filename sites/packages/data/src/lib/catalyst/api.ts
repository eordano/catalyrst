import { z } from "zod";

import { getJSON } from "./client";
import type { GetOptions } from "./client";
import { placesApiPath } from "./typed";
import { ApiDataSchema, ApiDataTotalSchema } from "./generated-schemas/places";
import { parsePlace, parsePlaces, parseCategory, reportSchemaDrift } from "./schema";
import type { PlacesSource } from "./types";
import type { Category } from "./schema";

const ListEnvelope = ApiDataTotalSchema(z.unknown());
const DetailEnvelope = ApiDataSchema(z.unknown());
const CategoriesEnvelope = ApiDataSchema(z.array(z.unknown()));

const LooseEnvelope = z.object({
  data: z.unknown().optional(),
  total: z.number().optional(),
});

function looseData(raw: unknown): { data: unknown; total: number } {
  const r = LooseEnvelope.safeParse(raw);
  return { data: r.success ? r.data.data : undefined, total: r.success ? r.data.total ?? 0 : 0 };
}

export const apiSource: PlacesSource = {
  async fetchPlaces(params = {}, opts = {}) {
    const categories = Array.isArray(params.categories)
      ? params.categories.join(",")
      : params.categories;
    const raw = await getJSON<unknown>(placesApiPath("get", "/api/places"), {
      ...opts,
      query: {
        limit: params.limit,
        offset: params.offset,
        categories,
        search: params.search,
        owner: params.owner,
      },
    });
    const env = ListEnvelope.safeParse(raw);
    if (env.success) {
      return { data: parsePlaces(env.data.data), total: env.data.total };
    }
    reportSchemaDrift("PlacesListEnvelope", env.error.issues);
    const loose = looseData(raw);
    return {
      data: parsePlaces(Array.isArray(loose.data) ? loose.data : []),
      total: loose.total,
    };
  },

  async fetchPlace(id, opts = {}) {
    const raw = await getJSON<unknown>(
      placesApiPath("get", "/api/places/{place_id}", { place_id: id }),
      opts,
    );
    const env = DetailEnvelope.safeParse(raw);
    if (env.success) return parsePlace(env.data.data);
    reportSchemaDrift("PlaceDetailEnvelope", env.error.issues);
    return parsePlace(looseData(raw).data);
  },

  async fetchCategories(opts = {}) {
    const raw = await getJSON<unknown>(placesApiPath("get", "/api/categories"), opts);
    const env = CategoriesEnvelope.safeParse(raw);
    if (env.success) return env.data.data.map(parseCategory).filter((c): c is Category => c !== null);
    reportSchemaDrift("CategoriesEnvelope", env.error.issues);
    const loose = looseData(raw);
    return (Array.isArray(loose.data) ? loose.data : []).map(parseCategory).filter((c): c is Category => c !== null);
  },
};
