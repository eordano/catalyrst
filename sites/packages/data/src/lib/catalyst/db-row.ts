import type { Place } from "./schema";

function str(v: unknown): string | null {
  if (v instanceof Date) return v.toISOString();
  return typeof v === "string" ? v : null;
}

function num(v: unknown): number | null {
  return typeof v === "number" ? v : null;
}

function strs(v: unknown): string[] {
  return Array.isArray(v)
    ? v.filter((s): s is string => typeof s === "string")
    : [];
}

export function placeFromDbRow(row: Record<string, unknown>): Place {
  return {
    id: String(row.id ?? ""),
    title: str(row.title),
    description: str(row.description),
    image: str(row.image),
    owner: str(row.owner),
    positions: strs(row.positions),
    base_position: str(row.base_position) ?? "",
    updated_at: str(row.updated_at),
    created_at: str(row.created_at),
    contact_name: str(row.contact_name),
    categories: strs(row.categories),
    highlighted: row.highlighted === true,
    highlighted_image: str(row.highlighted_image),
    user_count: num(row.user_count),
    user_visits: num(row.user_visits) ?? 0,
    favorites: num(row.favorites) ?? 0,
    likes: num(row.likes) ?? 0,
    like_rate: num(row.like_rate),
    world: row.world === true,
    world_name: str(row.world_name),
  };
}
