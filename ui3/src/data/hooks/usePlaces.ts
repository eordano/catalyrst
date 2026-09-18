import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useBridgeState } from "../../overlay/bridge";
import { playScreenEnabled, playSection } from "../screens/play-client";

import { fetchPlaces, fetchPlace, fetchCategories, fetchWorlds } from "../catalyst/placesSchema";
import { qk, STALE } from "../queryKeys";
import type { QueryParams } from "../catalyst/client";
import { usePublicResult } from "./usePublicResult";
import { PLAY_PLACES_PARAMS } from "../screens/play";

export function usePlaces(params?: QueryParams, enabled = true, authenticated = false) {
  const client = useQueryClient();
  const address = useBridgeState((state) => state.identity.address);
  const keys = Object.entries(params ?? {}).filter(([, value]) => value !== undefined).map(([key]) => key);
  const featured = keys.length === 3 && params?.limit === 6 && params.only_highlighted === true && params.order_by === "most_active";
  const places = keys.length === 3 && (params?.limit === 60 || params?.limit === 48 || params?.limit === 24) && params.order_by === "most_active" && params.order === "desc";
  const key = authenticated ? [...qk.places(params), address] : qk.places(params);
  const query = useQuery({
    queryKey: key,
    queryFn: ({ signal }) => !authenticated && playScreenEnabled(client) && (featured || places)
      ? playSection(client, address, featured ? "featured" : "places", signal).then((rows) => rows.slice(0, Number(params?.limit)))
      : fetchPlaces(params, { signal, authenticated }),
    staleTime: STALE.places,
    enabled,
  });
  return usePublicResult(query, key, enabled && !authenticated && !params?.owner && !params?.only_favorites, places ? qk.places(PLAY_PLACES_PARAMS) : key);
}

export function useWorlds(params?: QueryParams, enabled = true) {
  const query = useQuery({
    queryKey: qk.worlds(params),
    queryFn: ({ signal }) => fetchWorlds(params, { signal }),
    staleTime: STALE.worlds,
    enabled,
  });
  return usePublicResult(query, qk.worlds(params), enabled);
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
