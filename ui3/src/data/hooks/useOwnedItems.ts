import { useQuery, type QueryClient } from "@tanstack/react-query";

import { qk, STALE } from "../queryKeys";
import {
  loadBackpack,
  loadBackpackEmotes,
  loadOutfits,
  normalizeAddress,
} from "../catalyst/backpack";

function keyAddr(address?: string | null): string {
  return normalizeAddress(address) || "anon";
}

export function useOwnedWearables(address?: string | null) {
  return useQuery({
    queryKey: qk.wearables(keyAddr(address)),
    queryFn: ({ signal }) => loadBackpack(address, { signal }),
    staleTime: STALE.wearables,
  });
}

export function useOwnedEmotes(address?: string | null) {
  return useQuery({
    queryKey: qk.emotes(keyAddr(address)),
    queryFn: ({ signal }) => loadBackpackEmotes(address, { signal }),
    staleTime: STALE.emotes,
  });
}

export function useOutfits(address?: string | null) {
  return useQuery({
    queryKey: qk.outfits(keyAddr(address)),
    queryFn: ({ signal }) => loadOutfits(address, { signal }),
    staleTime: STALE.outfits,
  });
}

export function useOwnedItems(address?: string | null) {
  const wearables = useOwnedWearables(address);
  const emotes = useOwnedEmotes(address);
  return {
    wearables,
    emotes,
    isLoading: wearables.isLoading || emotes.isLoading,
    isFetching: wearables.isFetching || emotes.isFetching,
    isError: wearables.isError || emotes.isError,
    error: wearables.error ?? emotes.error ?? null,
  };
}

export function prefetchOwnedItems(queryClient: QueryClient, address?: string | null) {
  queryClient.prefetchQuery({
    queryKey: qk.wearables(keyAddr(address)),
    queryFn: ({ signal }) => loadBackpack(address, { signal }),
    staleTime: STALE.wearables,
  });
  queryClient.prefetchQuery({
    queryKey: qk.emotes(keyAddr(address)),
    queryFn: ({ signal }) => loadBackpackEmotes(address, { signal }),
    staleTime: STALE.emotes,
  });
}
