import { z } from "zod";

const nullableStr = z.string().nullish().transform((v) => v ?? null);

export const PlaceRefSchema = z.object({
  id: z.string(),
  title: nullableStr,
  base_position: z.string(),
  parcels: z.number().nullish().transform((v) => v ?? null),
  contact_name: nullableStr,
  image: nullableStr,
  user_count: z.number(),
});

export type PlaceRef = z.infer<typeof PlaceRefSchema>;

const BanRowSchema = z.object({
  bannedAddress: z.string(),
  name: nullableStr,
});

z.object({
  results: z.array(BanRowSchema),
  total: z.number(),
  page: z.number(),
  pages: z.number(),
  limit: z.number(),
});

