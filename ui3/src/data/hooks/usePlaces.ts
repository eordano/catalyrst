import { useQuery } from "@tanstack/react-query";

import { fetchPlaces, fetchPlace, fetchCategories, fetchWorlds } from "../catalyst/placesSchema";
import { qk, STALE } from "../queryKeys";
import type { QueryParams } from "../catalyst/client";

export function usePlaces(params?: QueryParams, enabled = true) {
  return useQuery({
    queryKey: qk.places(params),
    queryFn: ({ signal }) => fetchPlaces(params, { signal }),
    staleTime: STALE.places,
    enabled,
  });
}

export function useWorlds(params?: QueryParams, enabled = true) {
  return useQuery({
    queryKey: qk.worlds(params),
    queryFn: ({ signal }) => fetchWorlds(params, { signal }),
    staleTime: STALE.worlds,
    enabled,
  });
}

export function usePlace(id?: string | null) {
  return useQuery({
    queryKey: qk.place(id),
    queryFn: ({ signal }) => fetchPlace(id, { signal }),
    enabled: Boolean(id),
    staleTime: STALE.place,
  });
}

export function useCategories() {
  return useQuery({
    queryKey: qk.categories(),
    queryFn: ({ signal }) => fetchCategories({ signal }),
    staleTime: STALE.categories,
  });
}
