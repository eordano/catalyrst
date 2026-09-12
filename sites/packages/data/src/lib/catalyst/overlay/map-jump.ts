import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { ApiDataTotalSchema } from "../generated-schemas/places";

export const MAP_PIN_LIMIT = 40;

export const PIN_CATEGORIES = [
  { key: "all", label: "All" },
  { key: "live", label: "Live Events" },
  { key: "poi", label: "POI" },
  { key: "minigames", label: "Mini-Games" },
  { key: "people", label: "People" },
] as const;

export type PinCategory = (typeof PIN_CATEGORIES)[number]["key"];

const PIN_CATEGORY_KEYS = new Set<string>(PIN_CATEGORIES.map((c) => c.key));

export function normalizePinCategory(raw: string | null | undefined): PinCategory {
  const v = (raw ?? "").trim().toLowerCase();
  return (PIN_CATEGORY_KEYS.has(v) ? v : "all") as PinCategory;
}

const nullableStr = z.string().nullish().transform((v) => v ?? null);

export const PlaceRowSchema = z.object({
  id: z.string(),
  title: nullableStr,
  base_position: z.string(),
  categories: z.array(z.string()).nullish().transform((v) => v ?? null),
  user_count: z.number().nullish().transform((v) => v ?? null),
  like_rate: z.number().nullish().transform((v) => v ?? null),
  contact_name: nullableStr,
  owner: nullableStr,
  highlighted: z.boolean().nullish().transform((v) => v ?? false),
  world: z.boolean().nullish().transform((v) => v ?? null),
  world_name: nullableStr,
  image: nullableStr,
});

type PlaceRow = z.infer<typeof PlaceRowSchema>;

const PlacesEnvelopeSchema = ApiDataTotalSchema(z.unknown());

export type MapPin = {
  id: string;
  name: string;
  coords: string;
  x: number;
  y: number;
  category: PinCategory;
  users: number | null;
  rating: number;
  live: boolean;
  featured: boolean;
  creator: string;
  world: boolean | null;
  worldName: string | null;
  image: string | null;
};

export type MapJumpData = {
  pins: MapPin[];
  source: "catalyst" | "unavailable";
  reason?: string;
};

export function unavailableMapJump(reason: string): MapJumpData {
  return { pins: [], source: "unavailable", reason };
}

export function parseCoords(pos: string): [number, number] {
  const [xs, ys] = (pos || "0,0").split(",");
  const x = Number.parseInt((xs ?? "0").trim(), 10);
  const y = Number.parseInt((ys ?? "0").trim(), 10);
  return [Number.isFinite(x) ? x : 0, Number.isFinite(y) ? y : 0];
}

function pinCategoryFor(row: PlaceRow): PinCategory {
  const cats = (row.categories ?? []).map((c) => c.toLowerCase());
  if (row.user_count !== null && row.user_count > 0) return "live";
  if (cats.includes("poi") || cats.includes("featured")) return "poi";
  if (cats.includes("game") || cats.includes("parkour") || cats.includes("casino")) {
    return "minigames";
  }
  return "people";
}

export function rowToPin(row: PlaceRow): MapPin {
  const [x, y] = parseCoords(row.base_position);
  const users = row.user_count;
  return {
    id: row.id,
    name: row.title ?? "Untitled parcel",
    coords: row.base_position,
    x,
    y,
    category: pinCategoryFor(row),
    users,
    rating: Math.round((row.like_rate ?? 0) * 100),
    live: users !== null && users > 0,
    featured: row.highlighted,
    creator: (row.contact_name ?? row.owner ?? "").trim(),
    world: row.world,
    worldName: row.world_name,
    image: row.image,
  };
}

export async function loadMapPins(opts: GetOptions = {}): Promise<MapJumpData> {
  const env = await getJSON<unknown>("/places/api/places", {
    ...opts,
    query: { limit: MAP_PIN_LIMIT, ...(opts.query ?? {}) },
  });
  const parsed = PlacesEnvelopeSchema.safeParse(env);
  if (!parsed.success) {
    throw new Error("/places/api/places did not return a data array");
  }

  const pins: MapPin[] = [];
  for (const raw of parsed.data.data) {
    const row = PlaceRowSchema.safeParse(raw);
    if (row.success) pins.push(rowToPin(row.data));
  }
  return { pins, source: "catalyst" };
}

export function filterPins(pins: MapPin[], category: PinCategory): MapPin[] {
  if (category === "all") return pins;
  return pins.filter((p) => p.category === category);
}

export function findPinByCoords(pins: MapPin[], coords: string | null | undefined): MapPin | null {
  if (!coords) return null;
  const want = coords.trim();
  return pins.find((p) => p.coords === want) ?? null;
}

export function isWorldPin(pin: MapPin): boolean {
  return pin.worldName !== null && pin.world !== false;
}

export function buildJumpUrl(pin: MapPin): string {
  if (isWorldPin(pin) && pin.worldName) {
    return `https://catalyst.example.com/play/?realm=${encodeURIComponent(pin.worldName)}`;
  }
  const pos = (pin.coords || "0,0").trim();
  return `https://catalyst.example.com/play/?position=${pos}`;
}
