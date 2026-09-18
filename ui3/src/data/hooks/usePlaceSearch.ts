import { useEffect, useState } from "react";

import { usePlaces, useWorlds } from "./usePlaces";
import type { PlaceView } from "../catalyst/places";

const SEARCH_DEBOUNCE_MS = 280;
const MIN_QUERY_LEN = 2;
const ENS_RE = /\.eth$/i;

export function isEnsQuery(query: string): boolean {
  return ENS_RE.test(query.trim());
}

type PlaceSearchResult = {
  placeHits: PlaceView[];
  worldHits: PlaceView[];
  loading: boolean;
  active: boolean;
  error: boolean;
  retry: () => void;
};

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

  const waiting = query.trim() !== debounced;
  const searching = query.trim().length >= MIN_QUERY_LEN;
  return {
    placeHits: active && !waiting ? (placesQ.data ?? []) : [],
    worldHits: active && !waiting ? (worldsQ.data ?? []) : [],
    loading: searching && (waiting || placesQ.isPending || worldsQ.isPending),
    active: searching,
    error: searching && !waiting && (placesQ.isError || worldsQ.isError),
    retry: () => { void placesQ.refetch(); void worldsQ.refetch(); },
  };
}
