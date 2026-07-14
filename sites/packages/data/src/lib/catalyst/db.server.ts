import { Pool } from "pg";

import { CatalystError } from "./client";
import {
  PLACE_COLUMNS,
  CATEGORIES_SQL,
  buildListQuery,
  buildCountQuery,
} from "./db-query";
import { placeFromDbRow } from "./db-row";
import { parseCategory, categoryI18nEn } from "./schema";
import type { Place, Category } from "./schema";
import type { PlacesSource } from "./types";

function connectionString(): string | undefined {
  if (typeof process === "undefined") return undefined;
  return (
    process.env.CATALYST_DATABASE_URL ||
    process.env.PLACES_PG_COMPONENT_PSQL_CONNECTION_STRING ||
    undefined
  );
}

let pool: Pool | null = null;
function getPool(): Pool {
  if (!pool) {
    pool = new Pool({
      connectionString: connectionString(),
      max: 5,
      statement_timeout: 60_000,
      idle_in_transaction_session_timeout: 30_000,
      idleTimeoutMillis: 30_000,
    });
  }
  return pool;
}

export const dbSource: PlacesSource = {
  async fetchPlaces(params = {}): Promise<{ data: Place[]; total: number }> {
    const s = params.search?.trim();
    if (s && s.length > 0 && s.length < 3) return { data: [], total: 0 };

    const list = buildListQuery(params);
    const count = buildCountQuery(params);
    const db = getPool();
    const [listRes, countRes] = await Promise.all([
      db.query(list.text, list.values),
      db.query(count.text, count.values),
    ]);
    return {
      data: listRes.rows.map(placeFromDbRow),
      total: Number(countRes.rows[0]?.total ?? 0),
    };
  },

  async fetchPlace(id: string): Promise<Place> {
    const res = await getPool().query(
      `SELECT ${PLACE_COLUMNS} FROM place WHERE id = $1`,
      [id],
    );
    const row = res.rows[0];
    if (!row) {
      throw new CatalystError(`place ${id} not found`, "db://place", 404);
    }
    return placeFromDbRow(row);
  },

  async fetchCategories(): Promise<Category[]> {
    const res = await getPool().query(CATEGORIES_SQL);
    return res.rows.flatMap((r: { name: string; count: string | number }) => {
      const c = parseCategory({
        name: r.name,
        count: Number(r.count),
        active: true,
        i18n: { en: categoryI18nEn(r.name) },
      });
      return c ? [c] : [];
    });
  },
};
