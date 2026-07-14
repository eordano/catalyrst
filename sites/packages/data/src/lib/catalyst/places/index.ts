export type { Place, Category, Envelope } from "../schema";
export type { FetchPlacesParams, PlacesSource } from "../types";

import { z } from "zod";

import { apiSource } from "../api";
import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { placesApiPath } from "../typed";
import { ApiDataTotalSchema } from "../generated-schemas/places";
import { parsePlaces, reportSchemaDrift } from "../schema";
import type { Place } from "../schema";
import type { FetchPlacesParams } from "../types";

export function fetchPlaces(params?: FetchPlacesParams, opts?: GetOptions) {
  return apiSource.fetchPlaces(params, opts);
}
export function fetchPlace(id: string, opts?: GetOptions) {
  return apiSource.fetchPlace(id, opts);
}
export function fetchCategories(opts?: GetOptions) {
  return apiSource.fetchCategories(opts);
}

export type PlaceCardProps = {
  id: string;
  title: string;
  image?: string;
  players: number;
  rating: number;
  coords: string;
  live: boolean;
  featured: boolean;
  creator: string;
};

export function mapPlace(p: Place): PlaceCardProps {
  const players = p.user_count ?? 0;
  return {
    id: p.id,
    title: p.title ?? "",
    image: p.image ?? undefined,
    players,
    rating: Math.round((p.like_rate ?? 0) * 100),
    coords: p.base_position,
    live: players > 0,
    featured: p.highlighted,
    creator: p.contact_name ?? p.owner ?? "",
  };
}

export type ProfilePlace = {
  id: string;
  title: string;
  description: string;
  image?: string;
  base_position: string;
  positions: string[];
  world: boolean;
  world_name?: string;
  likes: number;
  favorites: number;
  user_count: number;
  contact_name: string;
};

function placeImageCss(image: string | null | undefined): string | undefined {
  const v = (image ?? "").trim();
  if (!v) return undefined;
  if (/^(url\(|(repeating-)?(linear|radial|conic)-gradient\()/i.test(v)) return v;
  return `url("${v.replace(/"/g, "%22")}")`;
}

export function mapProfilePlace(p: Place, fallbackName?: string): ProfilePlace {
  return {
    id: p.id,
    title: p.title ?? "",
    description: p.description ?? "",
    image: placeImageCss(p.image),
    base_position: p.base_position,
    positions: p.positions,
    world: p.world,
    world_name: p.world_name ?? undefined,
    likes: p.likes,
    favorites: p.favorites,
    user_count: p.user_count ?? 0,
    contact_name: p.contact_name?.trim() || fallbackName || "",
  };
}

const PlacesListEnvelope = ApiDataTotalSchema(z.unknown());

export async function fetchMostActivePlaces(
  params: { limit?: number } = {},
  opts: GetOptions = {},
): Promise<Place[]> {
  const raw = await getJSON<unknown>(placesApiPath("get", "/api/places"), {
    ...opts,
    query: {
      limit: params.limit ?? 4,
      order_by: "most_active",
      with_realtime_active_users: true,
    },
  });
  const env = PlacesListEnvelope.safeParse(raw);
  if (!env.success) {
    reportSchemaDrift("MostActivePlacesEnvelope", env.error.issues);
    const loose = z.object({ data: z.array(z.unknown()) }).safeParse(raw);
    return parsePlaces(loose.success ? loose.data.data : []);
  }
  return parsePlaces(env.data.data);
}

export async function fetchProfilePlaces(
  address: string,
  params: { limit?: number; fallbackName?: string } = {},
  opts: GetOptions = {},
): Promise<ProfilePlace[]> {
  const raw = await getJSON<unknown>(placesApiPath("get", "/api/places"), {
    ...opts,
    query: {
      creator_address: address,
      limit: params.limit ?? 100,
      order_by: "updated_at",
    },
  });
  const env = PlacesListEnvelope.safeParse(raw);
  if (!env.success) {
    reportSchemaDrift("ProfilePlacesEnvelope", env.error.issues);
    const loose = z.object({ data: z.array(z.unknown()) }).safeParse(raw);
    return parsePlaces(loose.success ? loose.data.data : []).map((p) =>
      mapProfilePlace(p, params.fallbackName),
    );
  }
  return parsePlaces(env.data.data).map((p) =>
    mapProfilePlace(p, params.fallbackName),
  );
}
