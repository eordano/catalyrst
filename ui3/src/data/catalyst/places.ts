import { catalystBase, sendSignedJSON, serviceBase } from "./client";
import { isRecord } from "./rows";
import type { PlaceWire, PlaceCategoryWire } from "./schemas/places";
import { hueFor } from "../format";

export const PLACES_LIMIT = 40;

// The wire -> `Place` normalization, which the schema module cannot carry: a
// perf build replaces it with an accepting stub, and a stub reproduces no
// transform. Here it runs whether or not validation did, so `players` is null
// for an unknown live count in both builds rather than undefined in one.
//
// Every field a schema marks nullish is restated, so the type below is exactly
// what a caller gets and a field added to the schema shows up as its honest
// `| undefined` until it is normalized here too.

export function normalizePlace(p: PlaceWire) {
  return {
    ...p,
    title: p.title ?? null,
    description: p.description ?? null,
    image: p.image ?? null,
    owner: p.owner ?? null,
    creator_address: p.creator_address ?? null,
    contact_name: p.contact_name ?? null,
    user_count: p.user_count ?? null,
    like_rate: p.like_rate ?? null,
    world_name: p.world_name ?? null,
    updated_at: p.updated_at ?? null,
  };
}

export type Place = ReturnType<typeof normalizePlace>;

/**
 * What `toPlaceView` needs, as opposed to what `PlaceSchema` declares -- four
 * fields of nineteen, and the reason it is not five is that the other fifteen
 * degrade on their own.
 *
 * `positions.length` and `categories.map` are unconditional reads, so a row
 * lacking either is a TypeError rather than a sparse card. `base_position` is
 * the parcel: `parseCoords` reads an absent one as 0,0, which asserts a location
 * nobody deployed -- schemas/places.ts says the same thing for the checking
 * build, and placesSchema.test.ts pins it. `id` keys the card, seeds its hue and
 * addresses the detail fetch.
 *
 * See rows.ts for why this is stated here rather than derived from the schema.
 */
export function isRenderablePlace(row: unknown): boolean {
  return (
    isRecord(row) &&
    typeof row.id === "string" &&
    typeof row.base_position === "string" &&
    Array.isArray(row.positions) &&
    Array.isArray(row.categories)
  );
}

/**
 * `i18n` is optional-chained because in perf mode the row reaching here was
 * never checked: `toCategoryView` reads `i18n.en` unconditionally, so making the
 * object total is what keeps one unlabelled row from taking the whole list down.
 */
export function normalizePlaceCategory(c: PlaceCategoryWire) {
  return { ...c, i18n: { en: c.i18n?.en ?? null } };
}

export type PlaceCategory = ReturnType<typeof normalizePlaceCategory>;

/**
 * `toCategoryView` keys the chip, labels it and colours it from `name`, so an
 * unnamed category is a blank chip that filters nothing.
 *
 * The one guard that is STRICTER than its schema, which it is allowed to be
 * only because the rejection already existed: `fetchCategories` tested
 * `r.success && r.data.name` by hand. Stating it here changes nothing in the
 * default build and makes it run in perf too.
 */
export function isRenderablePlaceCategory(row: unknown): boolean {
  return isRecord(row) && typeof row.name === "string" && row.name !== "";
}

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
  if (!iso) return "\u{2014}";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "\u{2014}";
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
