import { loadPlaces } from "../places/index.server";
import type { Place } from "../schema";

export type CreatorScene = {
  id: string;
  title: string;
  image: string | null;
  base_position: string;
  players: number;
  world: boolean;
  world_name: string | null;
  parcels: number;
  featured: boolean;
};

function toScene(p: Place): CreatorScene {
  return {
    id: p.id,
    title: p.title ?? "Untitled scene",
    image: p.image ?? null,
    base_position: p.base_position,
    players: p.user_count ?? 0,
    world: p.world ?? false,
    world_name: p.world_name ?? null,
    parcels: Array.isArray(p.positions) && p.positions.length > 0 ? p.positions.length : 1,
    featured: p.highlighted ?? false,
  };
}

export async function loadCreatorScenes(opts: {
  creator?: string;
  limit?: number;
} = {}): Promise<CreatorScene[]> {
  const limit = opts.limit ?? 12;
  const creator = opts.creator?.trim();
  if (!creator) return [];

  // A failed read propagates: every caller already separates "this creator has
  // no scenes" from "we could not ask", and an empty list here rendered the
  // creator's own scenes as gone.
  const needle = creator.toLowerCase();
  const { data } = await loadPlaces({ owner: creator, limit });
  if (data.length > 0) return data.slice(0, limit).map(toScene);

  const { data: window } = await loadPlaces({ limit: 60 });
  const places = window.filter((p) => {
    const owner = (p.owner ?? "").toLowerCase();
    const contact = (p.contact_name ?? "").toLowerCase();
    return owner === needle || contact === needle;
  });
  return places.slice(0, limit).map(toScene);
}
