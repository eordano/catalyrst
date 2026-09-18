import { z } from "zod";
import { warnInvalid } from "../warn";

z.object({
  id: z.string().nullish().transform((v) => v ?? null),
  place_id: z.string().nullish().transform((v) => v ?? null),
  added_by: z.string().nullish().transform((v) => v ?? null),
  created_at: z.number().nullish().transform((v) => v ?? null),
  active: z.boolean().nullish().transform((v) => v ?? null),
  admin: z.string(),
  name: z.string().nullish().transform((v) => v ?? null),
  canBeRemoved: z.boolean(),
});

const OperatedPlaceSchema = z.object({
  id: z.string(),
  title: z.string().nullish().transform((v) => v ?? null),
  base_position: z.string(),
  positions: z.array(z.string()),
  world: z.boolean(),
  world_name: z.string().nullish().transform((v) => v ?? null),
  owner: z.string().nullish().transform((v) => v ?? null),
  image: z.string().nullish().transform((v) => v ?? null),
});
export type OperatedPlace = z.infer<typeof OperatedPlaceSchema>;

export function parsePlaces(raw: unknown): OperatedPlace[] {
  if (!Array.isArray(raw)) {
    warnInvalid("OperatedPlace list", "not an array");
    return [];
  }
  const out: OperatedPlace[] = [];
  for (const item of raw) {
    const r = OperatedPlaceSchema.safeParse(item);
    if (r.success) out.push(r.data);
    else warnInvalid("OperatedPlace", r.error.issues);
  }
  return out;
}

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

