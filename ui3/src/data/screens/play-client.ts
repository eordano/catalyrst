import type { QueryClient } from "@tanstack/react-query";
import { loadBackpack, loadBackpackEmotes, loadOutfits, normalizeAddress } from "../catalyst/backpack";
import { fetchEvents } from "../catalyst/events";
import { fetchPlaces } from "../catalyst/placesSchema";
import { qk } from "../queryKeys";
import { PLAY_EVENTS_PARAMS, PLAY_UPCOMING_PARAMS, PLAY_FEATURED_PARAMS, PLAY_PLACES_PARAMS, type PlayScreen, type PlaySectionName, type PlaySectionData } from "./play";

import { readPlayStream } from "./play-stream";
import { publicFreshnessKey } from "../hooks/usePublicResult";

const hosts = new WeakMap<QueryClient, string>();
const unsupported = new WeakSet<QueryClient>();

function legacySection<K extends PlaySectionName>(section: K, address: string, signal?: AbortSignal) {
  const opts = { signal };
  const loaders = {
    wearables: () => loadBackpack(address, opts),
    emotes: () => loadBackpackEmotes(address, opts),
    outfits: () => loadOutfits(address, opts),
    events: () => fetchEvents(PLAY_EVENTS_PARAMS, opts),
    upcoming: () => fetchEvents(PLAY_UPCOMING_PARAMS, opts),
    featured: () => fetchPlaces(PLAY_FEATURED_PARAMS, opts),
    places: () => fetchPlaces(PLAY_PLACES_PARAMS, opts),
  };
  return loaders[section]() as Promise<PlaySectionData<K>>;
}

export function enablePlayScreen(client: QueryClient, base: string) {
  hosts.set(client, base.replace(/\/$/, ""));
}

export function playScreenEnabled(client: QueryClient) {
  return hosts.has(client);
}

function sectionKey(section: PlaySectionName, address: string) {
  return section === "wearables" ? qk.wearables(address || "anon")
    : section === "emotes" ? qk.emotes(address || "anon")
      : section === "outfits" ? qk.outfits(address || "anon")
      : section === "events" ? qk.events(PLAY_EVENTS_PARAMS)
        : section === "upcoming" ? qk.events(PLAY_UPCOMING_PARAMS)
        : qk.places(section === "featured" ? PLAY_FEATURED_PARAMS : PLAY_PLACES_PARAMS);
}

export async function playSection<K extends PlaySectionName>(
  client: QueryClient,
  address: string | null | undefined,
  section: K,
  signal?: AbortSignal,
): Promise<PlaySectionData<K>> {
  type Data = PlaySectionData<K>;
  signal?.throwIfAborted();
  const addr = normalizeAddress(address);
  if (unsupported.has(client)) return legacySection(section, addr, signal);
  const base = hosts.get(client);
  if (!base) throw new Error("Play screen data is not configured");
  const queryKey = sectionKey(section, addr);
  const before = client.getQueryState(queryKey)?.dataUpdateCount ?? 0;
  const screenKey = ["play-screen", base, addr];
  const previous = client.getQueryData<PlayScreen>(screenKey);
  const previousSection = previous?.sections[section];
  const refresh = client.getQueryState(queryKey)?.isInvalidated || previousSection?.status === "unavailable"
    || (previousSection?.status === "ready" && (previousSection.refreshing || previousSection.refreshFailed));
  let unsubscribe = () => {};
  let onAbort = () => {};
  const streamed = new Promise<Data>((resolve, reject) => {
    unsubscribe = client.getQueryCache().subscribe(() => {
      const state = client.getQueryState(queryKey);
      if (state && state.dataUpdateCount > before && !state.isInvalidated) resolve(state.data as Data);
    });
    onAbort = () => reject(signal?.reason);
    signal?.addEventListener("abort", onAbort, { once: true });
  });
  const complete = client.fetchQuery({
    queryKey: screenKey,
    staleTime: refresh ? 0 : 30_000,
    retry: false,
    queryFn: async ({ signal: requestSignal }) => {
      const started = Date.now();
      const counts = new Map<PlaySectionName, number>(
        (["featured", "places", "events", "upcoming", "wearables", "emotes", "outfits"] as const)
          .map((name) => [name, client.getQueryState(sectionKey(name, addr))?.dataUpdateCount ?? 0]),
      );
      const params = new URLSearchParams({ include: "upcoming" });
      if (addr) params.set("address", addr);
      const response = await fetch(`${base}/api/screens/v1/play?${params}`, {
        signal: AbortSignal.any([requestSignal, AbortSignal.timeout(5_000)]),
        headers: { accept: "application/x-ndjson" },
      });
      if (response.status === 404 || response.status === 501) {
        unsupported.add(client);
        throw new Error("This deployment does not provide play screen data");
      }
      return readPlayStream(response, addr, ({ section: name, result }) => {
        const key = sectionKey(name, addr);
        const existing = client.getQueryState(key);
        if (result.status === "ready" && (existing?.dataUpdateCount ?? 0) === counts.get(name)) {
          if (["featured", "places", "events", "upcoming"].includes(name)) {
            client.setQueryData(publicFreshnessKey(key), {
              updatedAt: result.updatedAt, refreshing: Boolean(result.refreshing), refreshFailed: Boolean(result.refreshFailed),
            });
          }
          client.setQueryData<unknown>(key, result.data, { updatedAt: started });
        }
      });
    },
  }).then((screen) => {
    const result = screen.sections[section];
    if (!result && section === "upcoming") return legacySection(section, addr, signal);
    if (!result) throw new Error("Missing play section");
    if (result.status !== "ready") throw new Error("This section is temporarily unavailable");
    return client.getQueryData<Data>(queryKey) ?? result.data as Data;
  });
  try {
    return await Promise.race([streamed, complete]);
  } catch (error) {
    signal?.throwIfAborted();
    if (unsupported.has(client)) return legacySection(section, addr, signal);
    throw error;
  } finally {
    unsubscribe();
    signal?.removeEventListener("abort", onAbort);
  }
}
