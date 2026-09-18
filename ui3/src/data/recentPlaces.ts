import type { PlaceView } from "./catalyst/places";
import { RecentPlacesSchema } from "./persisted-schemas";

const RECENT_KEY = "dcl.recentPlaces";

export function getRecent(): PlaceView[] {
  try {
    const result = RecentPlacesSchema.safeParse(JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]"));
    return result.success ? result.data.map((p) => ({ ...p, image: p.image })) : [];
  } catch { return []; }
}

export function pushRecent(place: PlaceView): void {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify([place, ...getRecent().filter((p) => p.id !== place.id)].slice(0, 24)));
  } catch { }
}
