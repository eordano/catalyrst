import { useEffect, useState } from "react";

import { usePlaces, useWorlds } from "./usePlaces";
import type { PlaceView } from "../catalyst/places";

const SEARCH_DEBOUNCE_MS = 280;
const MIN_QUERY_LEN = 2;
const ENS_RE = /\.eth$/i;

export function isEnsQuery(query: string): boolean {
  return ENS_RE.test(query.trim());
}

export type PlaceSearchResult = {
  placeHits: PlaceView[];
  worldHits: PlaceView[];
  loading: boolean;
  active: boolean;
};

// Debounced places+worlds search, shared by the Map search box, the Places grid and the
// post-login PlacesPicker. Worlds (e.g. boedo.dcl.eth) live off the Genesis atlas so they're
// queried separately from places; an `.eth` query is looked up by exact world name.
export function usePlaceSearch(query: string): PlaceSearchResult {
  const [debounced, setDebounced] = useState(query.trim());

  useEffect(() => {
    const t = setTimeout(() => setDebounced(query.trim()), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [query]);

  const active = debounced.length >= MIN_QUERY_LEN;
  const ens = isEnsQuery(debounced);

  const placesQ = usePlaces({ limit: 12, search: debounced }, active);
  const worldsQ = useWorlds(
    ens ? { limit: 12, names: debounced } : { limit: 12, search: debounced },
    active,
  );

  return {
    placeHits: active ? (placesQ.data ?? []) : [],
    worldHits: active ? (worldsQ.data ?? []) : [],
    loading: active && (placesQ.isLoading || worldsQ.isLoading),
    active,
  };
}
