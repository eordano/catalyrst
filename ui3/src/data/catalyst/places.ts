import { catalystBase, sendSignedJSON, serviceBase } from "./client";
import type { Place, PlaceCategory } from "./placesSchema";
import { hueFor } from "../format";

export type { Place, PlaceCategory };

export const PLACES_LIMIT = 40;

const GRID_MIN = -170;
const GRID_SPAN = 340;
const GRID_MAX = GRID_MIN + GRID_SPAN;

export function parseCoords(pos?: string | null): [number, number] {
  const [xs, ys] = String(pos ?? "0,0").split(",");
  const x = Number.parseInt((xs ?? "0").trim(), 10);
  const y = Number.parseInt((ys ?? "0").trim(), 10);
  return [Number.isFinite(x) ? x : 0, Number.isFinite(y) ? y : 0];
}

function clampPct(n: number): number {
  return Math.max(2, Math.min(98, n));
}

export function coordsToPercent(coords?: string | null): { left: number; top: number } {
  const [x, y] = parseCoords(coords);
  return {
    left: clampPct(((x - GRID_MIN) / GRID_SPAN) * 100),
    top: clampPct(((GRID_MAX - y) / GRID_SPAN) * 100),
  };
}

const CONTENT_IMAGE_PATH = /^\/content\/contents\//;
const MAP_IMAGE_PATH = /^\/v2\/map\.png$/;

export function localImageUrl(image?: string | null): string | undefined {
  if (!image) return undefined;
  try {
    const u = new URL(image);
    if (CONTENT_IMAGE_PATH.test(u.pathname)) {
      return `${catalystBase()}${u.pathname}${u.search}`;
    }
    if (MAP_IMAGE_PATH.test(u.pathname)) {
      return `${serviceBase("map")}${u.pathname}${u.search}`;
    }
  } catch {
  }
  return image;
}

function creatorOf(p: Place): string {
  return (p.contact_name || p.owner || p.creator_address || "Unknown creator").trim();
}


function fmtDate(iso?: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  return d.toLocaleDateString();
}

function pinKind(p: Place): string {
  if (p.user_count != null && p.user_count > 0) return "live";
  const c = p.categories.map((s) => s.toLowerCase());
  if (c.includes("poi") || c.includes("featured")) return "poi";
  if (c.includes("game") || c.includes("parkour") || c.includes("casino")) return "place";
  if (p.highlighted) return "fav";
  return "place";
}

const CAT_COLORS: Record<string, string> = {
  social: "#5db0ff",
  music: "#b07bff",
  art: "#ff8a5c",
  game: "#5fd38a",
  fashion: "#ff6fb5",
  education: "#ffd24d",
  shop: "#6ee0d2",
  sports: "#7d8cff",
  business: "#c0c8d6",
  crypto: "#f3ba2f",
  casino: "#ff4d6d",
  poi: "#ffb019",
  parkour: "#a14bff",
  featured: "#ffd700",
};

function catColor(name: string): string {
  return CAT_COLORS[name] || `hsl(${hueFor(name)} 70% 62%)`;
}

export function toPlaceView(p: Place) {
  const [x, y] = parseCoords(p.base_position);
  const { left, top } = coordsToPercent(p.base_position);
  const players = p.user_count;
  return {
    id: p.id,
    title: p.title || "Untitled parcel",
    description: p.description || "",
    image: localImageUrl(p.image),
    coords: p.base_position,
    x,
    y,
    left,
    top,
    players,
    live: players != null && players > 0,
    featured: p.highlighted,
    rating: Math.round((p.like_rate ?? 0) * 100),
    favorites: p.favorites,
    likes: p.likes,
    visits: p.user_visits,
    parcels: p.positions.length || 1,
    categories: p.categories,
    creator: creatorOf(p),
    world: p.world,
    worldName: p.world_name,
    updated: fmtDate(p.updated_at),
    hue: hueFor(p.id),
    kind: pinKind(p),
  };
}

export type PlaceView = ReturnType<typeof toPlaceView>;

export function toPlaceDetail(view: PlaceView | null | undefined) {
  if (!view) return null;
  return {
    id: view.id,
    title: view.title,
    coords: view.coords,
    parcels: view.parcels,
    favorites: view.favorites,
    views: view.visits,
    approval: view.rating,
    creator: view.creator,
    updated: view.updated,
    description: view.description || "No description provided.",
    hue: view.hue,
    image: view.image,
    world: view.world,
    worldName: view.worldName,
  };
}

export async function setPlaceFavorite(entityId: string, favorites: boolean): Promise<boolean> {
  if (!entityId) return false;
  const res = await sendSignedJSON(`/api/places/${encodeURIComponent(entityId)}/favorites`, {
    service: "places",
    method: "PATCH",
    body: { favorites },
  });
  return res != null;
}

export async function setPlaceLike(entityId: string, likes: boolean | null): Promise<boolean> {
  if (!entityId) return false;
  const res = await sendSignedJSON(`/api/places/${encodeURIComponent(entityId)}/likes`, {
    service: "places",
    method: "PATCH",
    body: { likes },
  });
  return res != null;
}

export function toCategoryView(c: PlaceCategory) {
  const label = (c.i18n.en || c.name || "").trim();
  return {
    key: c.name,
    name: c.name,
    label: label || c.name,
    count: c.count,
    color: catColor(c.name),
  };
}

export type CategoryView = ReturnType<typeof toCategoryView>;
