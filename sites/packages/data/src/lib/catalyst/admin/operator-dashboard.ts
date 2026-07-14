import { z } from "zod";

/**
 * Read against `catalyrst-places/src/ports/places/rows.rs:43-95` (`PlaceRow`),
 * which is what `GET /places/api/places` serialises, and mirrored by the
 * generated `generated-schemas/places.ts`.
 *
 * `highlighted`, `disabled` and `world` are `bool` there -- always present, so
 * they are required here and a row that omits one is dropped rather than
 * silently rendered as not-highlighted. `user_visits`, `likes`, `dislikes` and
 * `favorites` are `i32` and get the same treatment.
 *
 * `user_count` is `Option<i32>`: null is the honest reading of a place whose
 * headcount was not reported, and 0 is a measurement nobody took.
 *
 * `visits_24h`, `banned_count` and `admin_count` are NOT fields of `PlaceRow`
 * and never arrive. They used to default to 0, and the dashboard summed those
 * zeros into "24h visits", "Banned (total)" and "Admins (total)" KPI cards --
 * three figures with no source at all. They are gone from the schema and from
 * the page; nothing here may reintroduce them without a server field to read.
 *
 * `headcount` is likewise absent from `PlaceRow`. It stays as an explicit null
 * so the sparkline says the history is unavailable instead of drawing a flat
 * line of zeroes.
 *
 * `title` is `Option<String>` upstream, so null is the honest reading of a
 * place that has no title; callers fall back to the coordinates or the id.
 */
export const OperatorPlaceSchema = z.object({
  id: z.string(),
  title: z.string().nullish().transform((v) => v ?? null),
  base_position: z.string(),
  user_count: z.number().nullish().transform((v) => v ?? null),
  user_visits: z.number(),
  likes: z.number(),
  dislikes: z.number(),
  favorites: z.number(),
  like_rate: z.number().nullish().transform((v) => v ?? null),
  highlighted: z.boolean(),
  disabled: z.boolean(),
  world: z.boolean(),
  world_name: z.string().nullish().transform((v) => v ?? null),
  headcount: z.array(z.number()).nullish().transform((v) => v ?? null),
});
export type OperatorPlace = z.infer<typeof OperatorPlaceSchema>;

/**
 * The envelope around the place list is assembled by the loader, not parsed
 * from a payload: no endpoint on this node publishes a snapshot cadence, so
 * `snapshot_taken_at` and `snapshot_interval_min` are null rather than a
 * plausible "30 minutes" nobody configured.
 */
export const OperatorDashboardSchema = z.object({
  _source: z.string().optional(),
  owner: z.string(),
  owner_name: z.string().nullish().transform((v) => v ?? null),
  snapshot_taken_at: z.string().nullish().transform((v) => v ?? null),
  snapshot_interval_min: z.number().nullish().transform((v) => v ?? null),
  places: z.array(OperatorPlaceSchema),
});
export type OperatorDashboard = z.infer<typeof OperatorDashboardSchema>;

export const RANGES = ["1h", "6h", "24h"] as const;
export type Range = (typeof RANGES)[number];
export const DEFAULT_RANGE: Range = "24h";

export function rangePoints(range: Range): number {
  switch (range) {
    case "1h":
      return 2;
    case "6h":
      return 12;
    case "24h":
      return 48;
  }
}

export function coerceRange(raw: string | null | undefined): Range {
  return (RANGES as readonly string[]).includes(raw ?? "")
    ? (raw as Range)
    : DEFAULT_RANGE;
}

/**
 * `headcountUnreported` is part of the answer, not a diagnostic: the live-player
 * total only covers the places that reported one, and a reader has to be told
 * how many places are missing from it.
 */
export type DashboardTotals = {
  placeCount: number;
  totalLivePlayers: number;
  headcountUnreported: number;
  totalVisits: number;
  disabledCount: number;
};

export function totals(places: OperatorPlace[]): DashboardTotals {
  return places.reduce<DashboardTotals>(
    (acc, p) => ({
      placeCount: acc.placeCount + 1,
      totalLivePlayers: acc.totalLivePlayers + (p.user_count ?? 0),
      headcountUnreported: acc.headcountUnreported + (p.user_count == null ? 1 : 0),
      totalVisits: acc.totalVisits + p.user_visits,
      disabledCount: acc.disabledCount + (p.disabled ? 1 : 0),
    }),
    {
      placeCount: 0,
      totalLivePlayers: 0,
      headcountUnreported: 0,
      totalVisits: 0,
      disabledCount: 0,
    },
  );
}
