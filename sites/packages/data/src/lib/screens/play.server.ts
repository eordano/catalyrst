import { fetchEquipped, loadBackpack, loadBackpackEmotes, loadOutfits } from "@ui/data/catalyst/backpack";
import { fetchPlaces } from "@ui/data/catalyst/placesSchema";
import { fetchEvents } from "@ui/data/catalyst/events";
import { PLAY_EVENTS_PARAMS, PLAY_FEATURED_PARAMS, PLAY_PLACES_PARAMS, type PlayScreen, type PlaySectionMessage, type PlaySectionName } from "@ui/data/screens/play";
import { catalystBase } from "../catalyst/client";
import { withDeadline } from "../request-deadline";
import { screenSections } from "./section.server";

export function playAddress(request: Request) {
  const raw = new URL(request.url).searchParams.get("address")?.trim() ?? "";
  if (raw && !/^0x[0-9a-f]{40}$/i.test(raw)) throw new Response("Invalid address", { status: 400 });
  return raw.toLowerCase();
}

export async function loadPlayScreen(request: Request, onSection?: (message: PlaySectionMessage) => void) {
  const address = playAddress(request);
  const base = catalystBase();
  const sections = screenSections(request.signal);
  const equipped = withDeadline((signal) => fetchEquipped(address, { base, signal }), 3_000, request.signal);
  void equipped.catch(() => {});
  const publish = <K extends PlaySectionName>(section: K) => (result: PlayScreen["sections"][K]) => {
    onSection?.({ version: 1, address, section, result } as PlaySectionMessage);
    return result;
  };
  const [featured, places, events, wearables, emotes, outfits] = await Promise.all([
    sections.load("play_featured", (signal) => fetchPlaces(PLAY_FEATURED_PARAMS, { base: `${base}/places`, signal })).then(publish("featured")),
    sections.load("play_places", (signal) => fetchPlaces(PLAY_PLACES_PARAMS, { base: `${base}/places`, signal })).then(publish("places")),
    sections.load("play_events", (signal) => fetchEvents(PLAY_EVENTS_PARAMS, { base: `${base}/events`, signal })).then(publish("events")),
    sections.load("play_wearables", (signal) => loadBackpack(address, { base, signal, equipped })).then(publish("wearables")),
    sections.load("play_emotes", (signal) => loadBackpackEmotes(address, { base, signal, equipped })).then(publish("emotes")),
    sections.load("play_outfits", (signal) => loadOutfits(address, { base, signal })).then(publish("outfits")),
  ]);
  request.signal.throwIfAborted();
  const data: PlayScreen = { version: 1, address, sections: { featured, places, events, wearables, emotes, outfits } };
  return { data, serverTiming: sections.timing() };
}
