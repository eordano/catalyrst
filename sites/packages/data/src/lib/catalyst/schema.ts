import type { z } from "zod";

import { track } from "@core/lib/telemetry/track";
import {
  CategoryOutSchema,
  PlaceRowSchema,
} from "./generated-schemas/places";

export type PlaceRow = z.infer<typeof PlaceRowSchema>;
export type CategoryOut = z.infer<typeof CategoryOutSchema>;

export type Place = {
  id: string;
  title: string | null;
  description: string | null;
  image: string | null;
  owner: string | null;
  positions: string[];
  base_position: string;
  updated_at: string | null;
  created_at: string | null;
  contact_name: string | null;
  categories: string[];
  highlighted: boolean;
  highlighted_image: string | null;
  user_count: number | null;
  user_visits: number;
  favorites: number;
  likes: number;
  like_rate: number | null;
  world: boolean;
  world_name: string | null;
};

export type Category = {
  name: string;
  count: number;
  active: boolean;
  i18n: { en: string | null };
};

export type Envelope<T> = { ok: boolean; data: T; total: number };

export function normalizePlace(row: PlaceRow): Place {
  return {
    id: row.id,
    title: row.title,
    description: row.description,
    image: row.image,
    owner: row.owner,
    positions: row.positions,
    base_position: row.base_position,
    updated_at: row.updated_at,
    created_at: row.created_at,
    contact_name: row.contact_name,
    categories: row.categories,
    highlighted: row.highlighted,
    highlighted_image: row.highlighted_image,
    user_count: row.user_count,
    user_visits: row.user_visits,
    favorites: row.favorites,
    likes: row.likes,
    like_rate: row.like_rate,
    world: row.world,
    world_name: row.world_name,
  };
}

export function normalizeCategory(c: CategoryOut): Category {
  return {
    name: c.name,
    count: c.count,
    active: c.active,
    i18n: { en: c.i18n.en },
  };
}

export function reportSchemaDrift(kind: string, issues: readonly unknown[]): void {
  console.warn(`[catalyst] ${kind} failed schema validation`, issues);
  try {
    track(
      "catalyst_schema_drift",
      { kind, issues_count: issues.length },
      { sid: "catalyst-schema-guard" },
    );
  } catch {
  }
}

// A cast is not a check: once the schema has rejected the payload, `as Place`
// only stops the compiler from saying so, and every field it existed to
// guarantee -- base_position, user_count -- becomes whatever the upstream sent.
export function parsePlace(raw: unknown): Place | null {
  const r = PlaceRowSchema.safeParse(raw);
  if (r.success) return normalizePlace(r.data);
  reportSchemaDrift("Place", r.error.issues);
  return null;
}

export function parsePlaces(raw: unknown[]): Place[] {
  return (raw ?? []).map(parsePlace).filter((p): p is Place => p !== null);
}

export function parseCategory(raw: unknown): Category | null {
  const r = CategoryOutSchema.safeParse(raw);
  if (r.success) return normalizeCategory(r.data);
  reportSchemaDrift("Category", r.error.issues);
  return null;
}

const CATEGORY_I18N_EN: Record<string, string> = {
  poi: "\u{1F4CD} Point of Interest",
  featured: "\u{2728} Featured",
  art: "\u{1F3A8} Art",
  game: "\u{1F3AE} Game",
  casino: "\u{2663}\u{FE0F} Casino",
  social: "\u{1F465} Social",
  music: "\u{1F3B5} Music",
  fashion: "\u{1F460} Fashion",
  crypto: "\u{1FA99} Crypto",
  education: "\u{1F4DA} Education",
  shop: "\u{1F6CD}\u{FE0F} Shop",
  business: "\u{1F3E2} Business",
  sports: "\u{26BD}\u{FE0F} Sports",
  parkour: "\u{1F3C3} Parkour",
};

export function categoryI18nEn(name: string): string | null {
  return CATEGORY_I18N_EN[name] ?? null;
}
