import { afterEach, describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import { enablePlayScreen, playSection } from "./play-client";
import { PLAY_FEATURED_PARAMS, PLAY_EVENTS_PARAMS, PLAY_PLACES_PARAMS, type PlayScreen } from "./play";
import { qk } from "../queryKeys";
import { useOwnedWearables, useOwnedEmotes, useOutfits } from "../hooks/useOwnedItems";
import { usePlaces } from "../hooks/usePlaces";
import { useEvents } from "../hooks/useEvents";

const clients: QueryClient[] = [];
function client() {
  const value = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  clients.push(value);
  enablePlayScreen(value, "https://play.test");
  return value;
}
function data(address = ""): PlayScreen {
  const ready = <T,>(value: T) => ({ status: "ready" as const, data: value, updatedAt: Date.now() });
  return { version: 1, address, sections: {
    featured: ready([]), places: ready([]), events: ready({ data: [], total: 0 }),
    outfits: ready([]),
    wearables: ready({
      address, catalog: [], owned: [], ownedUrns: [], categories: [], ownedEmpty: true, source: "live",
      equipped: { name: "Guest", bodyShape: null, skinColor: null, hairColor: null, eyeColor: null, wearables: [], emotes: [], emoteSlots: [] },
    }),
    emotes: ready({ address: address || "anon", catalog: [], owned: [], ownedUrns: [], loadout: [], slotOrder: [], liveEmpty: true, source: "live" }),
  } };
}

afterEach(() => {
  cleanup();
  for (const value of clients.splice(0)) value.clear();
  vi.unstubAllGlobals();
});

describe("play screen client", () => {
  it("makes one request for the actual startup hooks and reuses their canonical cache entries", async () => {
    const qc = client();
    const fetch = vi.fn(async () => Response.json(data()));
    vi.stubGlobal("fetch", fetch);
    function Startup() {
      const wearables = useOwnedWearables();
      const emotes = useOwnedEmotes();
      const outfits = useOutfits();
      const featured = usePlaces(PLAY_FEATURED_PARAMS);
      const events = useEvents(PLAY_EVENTS_PARAMS);
      return <p>{[wearables, emotes, outfits, featured, events].every((query) => query.isSuccess) ? "ready" : "loading"}</p>;
    }
    const view = render(<QueryClientProvider client={qc}><Startup /></QueryClientProvider>);
    await screen.findByText("ready");
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith("https://play.test/api/screens/v1/play", expect.anything());
    expect(qc.getQueryData(qk.places(PLAY_PLACES_PARAMS))).toEqual([]);
    view.unmount();
    render(<QueryClientProvider client={qc}><Startup /></QueryClientProvider>);
    await screen.findByText("ready");
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("resolves a streamed section without waiting for the rest of the response", async () => {
    const qc = client();
    let controller!: ReadableStreamDefaultController<Uint8Array>;
    const body = new ReadableStream<Uint8Array>({ start(value) { controller = value; } });
    vi.stubGlobal("fetch", vi.fn(async () => new Response(body, { headers: { "content-type": "application/x-ndjson" } })));
    const send = (value: unknown) => controller.enqueue(new TextEncoder().encode(JSON.stringify(value) + "\n"));
    const pending = playSection(qc, null, "featured");
    send({ version: 1, address: "", section: "featured", result: data().sections.featured });
    expect(await pending).toEqual([]);
    expect(qc.getQueryData(qk.wearables("anon"))).toBeUndefined();
    for (const name of ["places", "events", "wearables", "emotes", "outfits"] as const) {
      send({ version: 1, address: "", section: name, result: data().sections[name] });
    }
    send({ version: 1, address: "", done: true });
    controller.close();
    await waitFor(() => expect(qc.getQueryData(["play-screen", "https://play.test", ""])).toBeDefined());
  });

  it("refreshes the aggregate after inventory invalidation instead of replaying stale inventory", async () => {
    const qc = client();
    const fetch = vi.fn(async () => Response.json(data()));
    vi.stubGlobal("fetch", fetch);
    await playSection(qc, null, "wearables");
    await waitFor(() => expect(qc.isFetching()).toBe(0));
    await qc.invalidateQueries({ queryKey: qk.wearables("anon"), refetchType: "none" });
    await playSection(qc, null, "wearables");
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("keeps addresses isolated and rejects a response for a different address", async () => {
    const qc = client();
    const a = "0x1111111111111111111111111111111111111111";
    const b = "0x2222222222222222222222222222222222222222";
    vi.stubGlobal("fetch", vi.fn(async (input: string) => Response.json(data(new URL(input).searchParams.get("address") ?? ""))));
    await Promise.all([playSection(qc, a, "wearables"), playSection(qc, b, "wearables")]);
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(qc.getQueryData(qk.wearables(a))).toMatchObject({ address: a });
    expect(qc.getQueryData(qk.wearables(b))).toMatchObject({ address: b });
    const other = client();
    vi.stubGlobal("fetch", vi.fn(async () => Response.json(data(a))));
    await expect(playSection(other, b, "wearables")).rejects.toThrow("Invalid play screen");
    expect(other.getQueryData(qk.wearables(b))).toBeUndefined();
  });

  it("does not replace a newer local update with a late response", async () => {
    const qc = client();
    let release!: (value: Response) => void;
    vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>((resolve) => { release = resolve; })));
    const pending = playSection(qc, null, "wearables");
    qc.setQueryData(qk.wearables("anon"), { ...data().sections.wearables.data!, marker: "newer" });
    release(Response.json(data()));
    expect(await pending).toMatchObject({ marker: "newer" });
    await waitFor(() => expect(qc.isFetching()).toBe(0));
    expect(qc.getQueryData(qk.wearables("anon"))).toMatchObject({ marker: "newer" });
    expect(await playSection(qc, null, "wearables")).toMatchObject({ marker: "newer" });
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("cancels one consumer without cancelling the shared response", async () => {
    const qc = client();
    let release!: (value: Response) => void;
    vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>((resolve) => { release = resolve; })));
    const cancel = new AbortController();
    const first = playSection(qc, null, "wearables", cancel.signal);
    const rejected = expect(first).rejects.toMatchObject({ name: "AbortError" });
    const second = playSection(qc, null, "emotes");
    cancel.abort();
    await rejected;
    release(Response.json(data()));
    expect(await second).toMatchObject({ liveEmpty: true });
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it("preserves partial failures as errors, and permits an immediate retry", async () => {
    const qc = client();
    const partial = data();
    partial.sections.events = { status: "unavailable", data: null, updatedAt: null };
    vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(Response.json(partial)).mockResolvedValueOnce(Response.json(data())));
    const failure = expect(playSection(qc, null, "events")).rejects.toThrow("unavailable");
    expect(await playSection(qc, null, "wearables")).toMatchObject({ ownedEmpty: true });
    await failure;
    expect(await playSection(qc, null, "events")).toEqual({ data: [], total: 0 });
    expect(fetch).toHaveBeenCalledTimes(2);
  });
});
