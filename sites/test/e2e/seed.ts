import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { Pool } from "pg";

type FixtureEntry = {
  id: string;
  title: string | null;
  description: string | null;
  creator_address: string | null;
  base_position: string;
  content_rating: string | null;
  disabled: boolean;
  favorites: number;
  likes: number;
  dislikes: number;
  categories: string[];
  highlighted: boolean;
  deployed_at: string;
  [k: string]: unknown;
};

const FIXTURE = fileURLToPath(
  new URL("../../packages/data/src/fixtures/places.json", import.meta.url),
);

export function loadFixtureEntries(): FixtureEntry[] {
  const parsed = JSON.parse(readFileSync(FIXTURE, "utf8")) as {
    places: { data: FixtureEntry[] };
  };
  return parsed.places.data;
}

export async function seedPlaces(
  pool: Pool,
  entries: FixtureEntry[] = loadFixtureEntries(),
): Promise<number> {
  for (const e of entries) {
    await pool.query(
      `INSERT INTO place
         (id, title, description, creator_address, base_position, content_rating,
          disabled, favorites, likes, dislikes, categories, highlighted,
          deployed_at, raw)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14::jsonb)
       ON CONFLICT (id) DO NOTHING`,
      [
        e.id,
        e.title,
        e.description,
        e.creator_address ?? null,
        e.base_position,
        e.content_rating ?? null,
        e.disabled,
        e.favorites,
        e.likes,
        e.dislikes,
        e.categories ?? [],
        e.highlighted,
        e.deployed_at,
        JSON.stringify(e),
      ],
    );
  }
  return entries.length;
}
