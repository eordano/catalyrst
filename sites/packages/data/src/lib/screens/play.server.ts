import { fetchEquipped, loadBackpack, loadBackpackEmotes, loadOutfits } from "@ui/data/catalyst/backpack";
import { fetchPlaces } from "@ui/data/catalyst/placesSchema";
import { fetchEvents } from "@ui/data/catalyst/events";
import { PLAY_EVENTS_PARAMS, PLAY_UPCOMING_PARAMS, PLAY_FEATURED_PARAMS, PLAY_PLACES_PARAMS, type PlayScreen, type PlaySectionMessage, type PlaySectionName } from "@ui/data/screens/play";
import { catalystBase } from "../catalyst/client";
import { withDeadline } from "../request-deadline";
import { publicFeedMemo, type PublicFeed } from "../public-feed.server";
import { screenSections } from "./section.server";

function publicFeed<T>(load: (base: string, signal: AbortSignal) => Promise<T>) {
  return publicFeedMemo({
    keyOf: (base: string) => base,
    load: (base) => withDeadline((signal) => load(base, signal), 3_000),
  });
}

const featuredFeed = publicFeed((base, signal) => fetchPlaces(PLAY_FEATURED_PARAMS, { base: `${base}/places`, signal }));
const placesFeed = publicFeed((base, signal) => fetchPlaces(PLAY_PLACES_PARAMS, { base: `${base}/places`, signal }));
const eventsFeed = publicFeed((base, signal) => fetchEvents(PLAY_EVENTS_PARAMS, { base: `${base}/events`, signal }));
const upcomingFeed = publicFeed((base, signal) => fetchEvents(PLAY_UPCOMING_PARAMS, { base: `${base}/events`, signal }));

export function resetPlayScreenCache() {
  featuredFeed.reset();
  placesFeed.reset();
  eventsFeed.reset();
  upcomingFeed.reset();
}

export function playAddress(request: Request) {
  const raw = new URL(request.url).searchParams.get("address")?.trim() ?? "";
  if (raw && !/^0x[0-9a-f]{40}$/i.test(raw)) throw new Response("Invalid address", { status: 400 });
  return raw.toLowerCase();
}

export async function loadPlayScreen(request: Request, onSection?: (message: PlaySectionMessage) => void) {
  const address = playAddress(request);
  const base = catalystBase();
  const sections = screenSections(request.signal);
  async function shared<T>(name: string, load: (base: string) => Promise<PublicFeed<T>>) {
    const result = await sections.load(name, () => load(base));
    return result.status === "ready" ? { status: "ready" as const, ...result.data } : result;
  }
  const equipped = withDeadline((signal) => fetchEquipped(address, { base, signal }), 3_000, request.signal);
  void equipped.catch(() => {});
  const publish = <K extends PlaySectionName>(section: K) => (result: NonNullable<PlayScreen["sections"][K]>) => {
    onSection?.({ version: 1, address, section, result } as PlaySectionMessage);
    return result;
  };
  const [featured, places, events, wearables, emotes, outfits, upcoming] = await Promise.all([
    shared("play_featured", featuredFeed).then(publish("featured")),
    shared("play_places", placesFeed).then(publish("places")),
    shared("play_events", eventsFeed).then(publish("events")),
    sections.load("play_wearables", (signal) => loadBackpack(address, { base, signal, equipped })).then(publish("wearables")),
    sections.load("play_emotes", (signal) => loadBackpackEmotes(address, { base, signal, equipped })).then(publish("emotes")),
    sections.load("play_outfits", (signal) => loadOutfits(address, { base, signal })).then(publish("outfits")),
    new URL(request.url).searchParams.get("include") === "upcoming"
      ? shared("play_upcoming", upcomingFeed).then(publish("upcoming")) : undefined,
  ]);
  request.signal.throwIfAborted();
  const data: PlayScreen = { version: 1, address, sections: { featured, places, events, wearables, emotes, outfits, ...(upcoming ? { upcoming } : {}) } };
  return { data, serverTiming: sections.timing() };
}
