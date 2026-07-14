import { apiSource } from "../api";
import { dbSource } from "../db.server";
import type { GetOptions } from "../client";
import type { Place, Category } from "../schema";
import type { FetchPlacesParams } from "../types";

function dbConfigured(): boolean {
  if (typeof process === "undefined") return false;
  return Boolean(
    process.env.CATALYST_DATABASE_URL ||
      process.env.PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING,
  );
}

let warned = false;
function warnFallback(err: unknown): void {
  if (warned) return;
  warned = true;
  console.warn(
    "[catalyst] direct DB read failed; falling back to HTTP API:",
    err instanceof Error ? err.message : err,
  );
}

export async function loadPlaces(
  params: FetchPlacesParams = {},
  opts: GetOptions = {},
): Promise<{ data: Place[]; total: number }> {
  if (dbConfigured()) {
    try {
      return await dbSource.fetchPlaces(params);
    } catch (err) {
      warnFallback(err);
    }
  }
  return apiSource.fetchPlaces(params, opts);
}

export async function loadPlace(
  id: string,
  opts: GetOptions = {},
): Promise<Place | null> {
  if (dbConfigured()) {
    try {
      return await dbSource.fetchPlace(id);
    } catch (err) {
      warnFallback(err);
    }
  }
  return apiSource.fetchPlace(id, opts);
}

const CATEGORIES_TTL_MS = 5 * 60_000;
let categoriesCache: { at: number; value: Category[] } | null = null;

export async function loadCategories(
  opts: GetOptions = {},
): Promise<Category[]> {
  if (categoriesCache && Date.now() - categoriesCache.at < CATEGORIES_TTL_MS) {
    return categoriesCache.value;
  }
  if (dbConfigured()) {
    try {
      const value = await dbSource.fetchCategories();
      categoriesCache = { at: Date.now(), value };
      return value;
    } catch (err) {
      warnFallback(err);
    }
  }
  const value = await apiSource.fetchCategories(opts);
  if (value.length > 0) categoriesCache = { at: Date.now(), value };
  return value;
}
