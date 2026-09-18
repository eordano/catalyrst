import type { loadBackpack, loadBackpackEmotes, loadOutfits } from "../catalyst/backpack";
import type { fetchEvents } from "../catalyst/events";
import type { PlaceView } from "../catalyst/places";
import type { ScreenSection } from "./section";

export const PLAY_FEATURED_PARAMS = { limit: 6, only_highlighted: true, order_by: "most_active" } as const;
export const PLAY_EVENTS_PARAMS = { list: "live", limit: 12 } as const;
export const PLAY_UPCOMING_PARAMS = { list: "upcoming", limit: 6 } as const;
export const PLAY_PLACES_PARAMS = { limit: 60, order_by: "most_active", order: "desc" } as const;
export const PLAY_LOBBY_PLACES_PARAMS = { ...PLAY_PLACES_PARAMS, limit: 24 } as const;

export type PlayScreen = {
  version: 1;
  address: string;
  sections: {
    featured: ScreenSection<PlaceView[]>;
    places: ScreenSection<PlaceView[]>;
    events: ScreenSection<Awaited<ReturnType<typeof fetchEvents>>>;
    upcoming?: ScreenSection<Awaited<ReturnType<typeof fetchEvents>>>;
    wearables: ScreenSection<Awaited<ReturnType<typeof loadBackpack>>>;
    emotes: ScreenSection<Awaited<ReturnType<typeof loadBackpackEmotes>>>;
    outfits: ScreenSection<Awaited<ReturnType<typeof loadOutfits>>>;
  };
};

export type PlaySectionName = keyof PlayScreen["sections"];
export type PlaySectionData<K extends PlaySectionName> = NonNullable<NonNullable<PlayScreen["sections"][K]>["data"]>;
export type PlaySectionMessage = {
  [K in PlaySectionName]-?: { version: 1; address: string; section: K; result: NonNullable<PlayScreen["sections"][K]> }
}[PlaySectionName];
