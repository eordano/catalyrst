import { z } from "zod";

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

const OperatorDashboardSchema = z.object({
  _source: z.string().optional(),
  owner: z.string(),
  owner_name: z.string().nullish().transform((v) => v ?? null),
  snapshot_taken_at: z.string().nullish().transform((v) => v ?? null),
  snapshot_interval_min: z.number().nullish().transform((v) => v ?? null),
  places: z.array(OperatorPlaceSchema),
});
export type OperatorDashboard = z.infer<typeof OperatorDashboardSchema>;

const RANGES = ["1h", "6h", "24h"] as const;
export type Range = (typeof RANGES)[number];
const DEFAULT_RANGE: Range = "24h";

export function coerceRange(raw: string | null | undefined): Range {
  return (RANGES as readonly string[]).includes(raw ?? "")
    ? (raw as Range)
    : DEFAULT_RANGE;
}

type DashboardTotals = {
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
