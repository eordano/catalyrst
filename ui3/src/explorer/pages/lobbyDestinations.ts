import type { PlaceView } from "../../data/catalyst/places";
import type { DclEvent } from "../../data/catalyst/events";

export type LobbyDestination = { name: string; realm: string } | { name: string; x: number; y: number };

export function parcelDestination(name: string, coords: string): LobbyDestination | null {
  if (!/^-?\d+\s*,\s*-?\d+$/.test(coords)) return null;
  const [x, y] = coords.split(",").map(Number);
  return Number.isSafeInteger(x) && Number.isSafeInteger(y) ? { name, x: x!, y: y! } : null;
}

export function placeDestination(place: PlaceView): LobbyDestination | null {
  if (place.world) return place.worldName ? { name: place.title, realm: place.worldName } : null;
  return parcelDestination(place.title, place.coords);
}

export function eventDestination(event: DclEvent): LobbyDestination | null {
  if (event.world) return event.server ? { name: event.name || "Event", realm: event.server } : null;
  if (event.x == null || event.y == null) return null;
  return parcelDestination(event.name || "Event", `${event.x},${event.y}`);
}
