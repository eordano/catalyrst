import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { playScreenEnabled, playSection } from "../screens/play-client";
import { prefetchImages } from "../prefetchImages";
import { firstBackpackPage } from "../catalyst/backpack-order";

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
  const client = useQueryClient();
  return useQuery({
    queryKey: qk.wearables(keyAddr(address)),
    queryFn: ({ signal }) => readWearables(client, address, signal),
    staleTime: STALE.wearables,
  });
}

export function useOwnedEmotes(address?: string | null) {
  const client = useQueryClient();
  return useQuery({
    queryKey: qk.emotes(keyAddr(address)),
    queryFn: ({ signal }) => readEmotes(client, address, signal),
    staleTime: STALE.emotes,
  });
}

export function useOutfits(address?: string | null) {
  const client = useQueryClient();
  return useQuery({
    queryKey: qk.outfits(keyAddr(address)),
    queryFn: ({ signal }) => playScreenEnabled(client)
      ? playSection(client, address, "outfits", signal) : loadOutfits(address, { signal }),
    staleTime: STALE.outfits,
  });
}

export function useOwnedItems(address?: string | null) {
  const wearables = useOwnedWearables(address);
  const emotes = useOwnedEmotes(address);
  return {
    wearables,
    emotes,
    isLoading: wearables.isPending || emotes.isPending,
    isFetching: wearables.isFetching || emotes.isFetching,
    isError: wearables.isError || emotes.isError,
    error: wearables.error ?? emotes.error ?? null,
  };
}

export function prefetchOwnedItems(queryClient: QueryClient, address?: string | null) {
  const wearables = queryClient.fetchQuery({
    queryKey: qk.wearables(keyAddr(address)),
    queryFn: ({ signal }) => readWearables(queryClient, address, signal),
    staleTime: STALE.wearables,
  }).then(data => {
    prefetchImages([
      ...firstBackpackPage(data.catalog).map(item => item.thumbnail),
      ...data.catalog.filter(item => data.equipped?.wearables?.includes(item.urn)).map(item => item.thumbnail),
    ]);
  });
  const emotes = queryClient.fetchQuery({
    queryKey: qk.emotes(keyAddr(address)),
    queryFn: ({ signal }) => readEmotes(queryClient, address, signal),
    staleTime: STALE.emotes,
  }).then(data => prefetchImages(firstBackpackPage(data.catalog).map(item => item.thumbnail)));
  return Promise.allSettled([wearables, emotes]);
}

export function readWearables(client: QueryClient, address?: string | null, signal?: AbortSignal) {
  return playScreenEnabled(client) ? playSection(client, address, "wearables", signal) : loadBackpack(address, { signal });
}

function readEmotes(client: QueryClient, address?: string | null, signal?: AbortSignal) {
  return playScreenEnabled(client) ? playSection(client, address, "emotes", signal) : loadBackpackEmotes(address, { signal });
}
