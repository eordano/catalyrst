import { useMemo } from "react";

import type { FriendEntry } from "../../generated/bridge/FriendEntry";
import type { NearbyPlayer } from "../../generated/bridge/NearbyPlayer";
import { useBridgeState } from "../../overlay/bridge";
import { coordsToPercent, parseCoords } from "../catalyst/places";

export type FriendPin = {
  address: string;
  name: string;
  coords: string;
  x: number;
  y: number;
  left: number;
  top: number;
  picture: string | null;
};

function isHttpUrl(s: unknown): s is string {
  return typeof s === "string" && /^https?:\/\//i.test(s);
}

// The friends push carries no coords and the players push carries no
// friendship, so a plottable friend is exactly their join: presence in the
// players push is the proof of being online on this island, which is the only
// place the engine can know a position from. Blocked-or-blocking friends stay
// in the friends push, so they must be dropped here or they get a named,
// jumpable pin on every map surface.
export function joinFriendPins(
  players: readonly NearbyPlayer[],
  friends: readonly FriendEntry[],
  blocked: readonly string[],
): FriendPin[] {
  if (players.length === 0 || friends.length === 0) return [];
  const blockedSet = new Set(blocked.map((a) => a.toLowerCase()));
  const nearby = new Map(players.map((p) => [p.address.toLowerCase(), p]));
  const pins: FriendPin[] = [];
  for (const f of friends) {
    if (blockedSet.has(f.address.toLowerCase())) continue;
    const p = nearby.get(f.address.toLowerCase());
    if (!p) continue;
    const [x, y] = parseCoords(p.coords);
    const { left, top } = coordsToPercent(p.coords);
    pins.push({
      address: p.address,
      name: f.name || p.name || "friend",
      coords: p.coords,
      x,
      y,
      left,
      top,
      picture: p.picture ?? (isHttpUrl(f.profilePictureUrl) ? f.profilePictureUrl : null),
    });
  }
  pins.sort((a, b) => a.address.localeCompare(b.address));
  return pins;
}

export function useFriendPins(): FriendPin[] {
  const players = useBridgeState((s) => s.players);
  const friends = useBridgeState((s) => s.friends.friends);
  const blocked = useBridgeState((s) => s.friends.blocked);
  return useMemo(
    () => joinFriendPins(players, friends, blocked),
    [players, friends, blocked],
  );
}
