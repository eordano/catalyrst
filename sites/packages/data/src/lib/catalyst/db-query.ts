import type { FetchPlacesParams } from "./types";

export type SqlQuery = { text: string; values: unknown[] };

export const PLACE_COLUMNS = `
  id, title, description, raw->>'image' AS image,
  creator_address AS owner,
  creator_address,
  COALESCE((SELECT array_agg(p::text) FROM jsonb_array_elements_text(raw->'positions') p), ARRAY[]::text[]) AS positions,
  base_position,
  raw->>'contact_name' AS contact_name,
  raw->>'contact_email' AS contact_email,
  content_rating,
  disabled,
  NULLIF(raw->>'disabled_at','')::timestamptz AS disabled_at,
  raw->>'disabled_reason' AS disabled_reason,
  NULLIF(raw->>'created_at','')::timestamptz AS created_at,
  NULLIF(raw->>'updated_at','')::timestamptz AS updated_at,
  favorites, likes, dislikes, categories,
  COALESCE((SELECT array_agg(t::text) FROM jsonb_array_elements_text(raw->'tags') t), ARRAY[]::text[]) AS tags,
  highlighted,
  raw->>'highlighted_image' AS highlighted_image,
  NULLIF(raw->>'ranking','')::float8 AS ranking,
  raw->>'sdk' AS sdk,
  deployed_at,
  COALESCE((raw->>'world')::bool, false) AS world,
  raw->>'world_name' AS world_name,
  raw->>'world_id' AS world_id,
  COALESCE((raw->>'is_private')::bool, false) AS is_private,
  NULLIF(raw->>'user_count','')::int AS user_count,
  COALESCE(NULLIF(raw->>'user_visits','')::int, 0) AS user_visits,
  NULLIF(raw->>'like_rate','')::float8 AS like_rate,
  NULLIF(raw->>'like_score','')::float8 AS like_score
`.trim();

export const CATEGORIES_SQL = `
  SELECT cat AS name, count(*)::bigint AS count
  FROM place p, unnest(p.categories) AS cat
  WHERE p.disabled IS FALSE
  GROUP BY cat
  ORDER BY count DESC, name ASC
`.trim();

const DEFAULT_ORDER = `NULLIF(raw->>'like_score','')::float8 DESC NULLS LAST, deployed_at DESC`;

function normalizeCategories(c?: string | string[]): string[] {
  if (!c) return [];
  const arr = Array.isArray(c) ? c : c.split(",");
  return arr.map((s) => s.trim()).filter(Boolean);
}

function clampInt(n: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, Math.floor(n)));
}

export function buildWhere(params: FetchPlacesParams): {
  clause: string;
  values: unknown[];
} {
  const clauses = ["disabled IS FALSE"];
  const values: unknown[] = [];
  let idx = 1;

  const cats = normalizeCategories(params.categories);
  if (cats.length) {
    clauses.push(`categories && $${idx}`);
    values.push(cats);
    idx += 1;
  }

  const search = params.search?.trim();
  if (search && search.length >= 3) {
    clauses.push(
      `(to_tsvector('english', coalesce(title,'') || ' ' || coalesce(description,'')) @@ plainto_tsquery('english', $${idx}) ` +
        `OR title ILIKE $${idx + 1} OR description ILIKE $${idx + 1})`,
    );
    values.push(search, `%${search}%`);
    idx += 2;
  }

  const owner = params.owner?.trim();
  if (owner) {
    clauses.push(`lower(creator_address) = $${idx}`);
    values.push(owner.toLowerCase());
    idx += 1;
  }

  return { clause: clauses.join(" AND "), values };
}

export function buildListQuery(params: FetchPlacesParams): SqlQuery {
  const { clause, values } = buildWhere(params);
  const limit = clampInt(params.limit ?? 100, 0, 100);
  const offset = Math.max(0, Math.floor(params.offset ?? 0));

  let order = DEFAULT_ORDER;
  const search = params.search?.trim();
  if (search && search.length >= 3) {
    const rankIdx = values.length + 1;
    order =
      `ts_rank_cd(to_tsvector('english', coalesce(title,'') || ' ' || coalesce(description,'')), ` +
      `plainto_tsquery('english', $${rankIdx}), 32) DESC, ${DEFAULT_ORDER}`;
    values.push(search);
  }

  const text =
    `SELECT ${PLACE_COLUMNS} FROM place WHERE ${clause} ` +
    `ORDER BY ${order} LIMIT ${limit} OFFSET ${offset}`;
  return { text, values };
}

export function buildCountQuery(params: FetchPlacesParams): SqlQuery {
  const { clause, values } = buildWhere(params);
  return {
    text: `SELECT count(*)::bigint AS total FROM place WHERE ${clause}`,
    values,
  };
}
