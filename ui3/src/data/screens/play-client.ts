import type { QueryClient } from "@tanstack/react-query";
import { normalizeAddress } from "../catalyst/backpack";
import { qk } from "../queryKeys";
import { PLAY_EVENTS_PARAMS, PLAY_FEATURED_PARAMS, PLAY_PLACES_PARAMS, type PlayScreen, type PlaySectionName } from "./play";

import { readPlayStream } from "./play-stream";

const hosts = new WeakMap<QueryClient, string>();

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
        : qk.places(section === "featured" ? PLAY_FEATURED_PARAMS : PLAY_PLACES_PARAMS);
}

export async function playSection<K extends PlaySectionName>(
  client: QueryClient,
  address: string | null | undefined,
  section: K,
  signal?: AbortSignal,
): Promise<NonNullable<PlayScreen["sections"][K]["data"]>> {
  type Data = NonNullable<PlayScreen["sections"][K]["data"]>;
  signal?.throwIfAborted();
  const addr = normalizeAddress(address);
  const base = hosts.get(client);
  if (!base) throw new Error("Play screen data is not configured");
  const queryKey = sectionKey(section, addr);
  const before = client.getQueryState(queryKey)?.dataUpdateCount ?? 0;
  const screenKey = ["play-screen", base, addr];
  const previous = client.getQueryData<PlayScreen>(screenKey);
  const refresh = client.getQueryState(queryKey)?.isInvalidated || previous?.sections[section].status === "unavailable";
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
        (["featured", "places", "events", "wearables", "emotes", "outfits"] as const)
          .map((name) => [name, client.getQueryState(sectionKey(name, addr))?.dataUpdateCount ?? 0]),
      );
      const response = await fetch(`${base}/api/screens/v1/play${addr ? `?address=${encodeURIComponent(addr)}` : ""}`, {
        signal: AbortSignal.any([requestSignal, AbortSignal.timeout(5_000)]),
        headers: { accept: "application/x-ndjson" },
      });
      return readPlayStream(response, addr, ({ section: name, result }) => {
        const key = sectionKey(name, addr);
        const existing = client.getQueryState(key);
        if (result.status === "ready" && (existing?.dataUpdateCount ?? 0) === counts.get(name)) {
          client.setQueryData<unknown>(key, result.data, { updatedAt: started });
        }
      });
    },
  }).then((screen) => {
    const result = screen.sections[section];
    if (result.status !== "ready") throw new Error("This section is temporarily unavailable");
    return client.getQueryData<Data>(queryKey) ?? result.data as Data;
  });
  try {
    return await Promise.race([streamed, complete]);
  } finally {
    unsubscribe();
    signal?.removeEventListener("abort", onAbort);
  }
}
