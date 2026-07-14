import type { GetOptions } from "./client";
import type { Place, Category } from "./schema";

export type FetchPlacesParams = {
  limit?: number;
  offset?: number;
  categories?: string | string[];
  search?: string;
  owner?: string;
};

export interface PlacesSource {
  fetchPlaces(
    params?: FetchPlacesParams,
    opts?: GetOptions,
  ): Promise<{ data: Place[]; total: number }>;
  fetchPlace(id: string, opts?: GetOptions): Promise<Place | null>;
  fetchCategories(opts?: GetOptions): Promise<Category[]>;
}
