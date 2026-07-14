import { type Place } from "@data/lib/catalyst/places/index";

// The minimal place shape the open-screen arms render/navigate to. A place is an
// "open place" only when it has a positive live reading -- `toOpenPlace` returns
// null otherwise, so the schema-honesty invariant (never surface a fabricated or
// zero reading as live) is enforced at the projection, not the render.
export type OpenPlace = {
  id: string;
  title: string | null;
  base_position: string;
  user_count: number;
};

export function toOpenPlace(p: Place | null | undefined): OpenPlace | null {
  if (!p || p.user_count === null || p.user_count <= 0) return null;
  return {
    id: p.id,
    title: p.title,
    base_position: p.base_position,
    user_count: p.user_count,
  };
}

// Pick the busiest live place (highest user_count) and a distinct "surprise"
// place (a random other live place, or the busiest itself when it is the only
// one). Returns nulls when nothing is live so the genesis arm redirects and the
// three-cards arm disables its live-scene cards. `rng` is injectable for tests.
export function selectLiveTargets(
  places: Place[] | null | undefined,
  rng: () => number = Math.random,
): { busiest: OpenPlace | null; surprise: OpenPlace | null } {
  const live = (places ?? []).filter((p) => (p.user_count ?? 0) > 0);
  if (live.length === 0) return { busiest: null, surprise: null };
  const busiest = toOpenPlace(
    live.reduce((a, b) => ((b.user_count ?? 0) > (a.user_count ?? 0) ? b : a)),
  );
  const others = busiest ? live.filter((p) => p.id !== busiest.id) : live;
  const surprise =
    others.length > 0 ? toOpenPlace(others[Math.floor(rng() * others.length)]) : busiest;
  return { busiest, surprise };
}
