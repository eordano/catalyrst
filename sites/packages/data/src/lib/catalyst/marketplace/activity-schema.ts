import { z } from "zod";

import { dataOf } from "../envelope";
import { SaleSchema } from "../generated-schemas/market";
import { warnInvalid } from "../warn";

export { SaleSchema };

const nullableStr = z.string().nullish().transform((v) => v ?? null);
const nullableNum = z.number().nullish().transform((v) => v ?? null);

/**
 * A settled sale, straight from catalyrst-market's `Sale`. Every field it
 * carries is a fact about a transfer that happened, so a row missing its price,
 * buyer or timestamp is not a sale with gaps -- `parseSale` drops it instead of
 * putting a nameless zero-MANA trade at the top of the activity feed.
 */
export type Sale = z.infer<typeof SaleSchema>;

export type SalesEnvelope = { data: Sale[]; total: number };

export const TradeSchema = z.object({
  id: z.string(),
  chain_id: nullableNum,
  created_at: nullableStr,
  effective_since: nullableStr,
  expires_at: nullableStr,
  network: nullableStr,
  signer: nullableStr,
  contract: nullableStr,
  type: nullableStr,
  checks: z.record(z.string(), z.unknown()).nullish().transform((v) => v ?? null),
});

export type Trade = z.infer<typeof TradeSchema>;

export const TradesEnvelopeSchema = dataOf(
  z
    .object({
      data: z.array(z.unknown()),
      count: z.number().nullish(),
    })
    .nullish(),
);

export function parseSale(raw: unknown): Sale | null {
  const r = SaleSchema.safeParse(raw);
  if (r.success) return r.data;
  warnInvalid("Sale", r.error.issues);
  return null;
}

export function parseSales(raw: unknown[]): Sale[] {
  const out: Sale[] = [];
  for (const row of raw ?? []) {
    const sale = parseSale(row);
    if (sale) out.push(sale);
  }
  return out;
}

export function parseTrade(raw: unknown): Trade | null {
  const r = TradeSchema.safeParse(raw);
  if (r.success) return r.data;
  warnInvalid("Trade", r.error.issues);
  return null;
}

export function parseTrades(raw: unknown[]): Trade[] {
  const out: Trade[] = [];
  for (const row of raw ?? []) {
    const trade = parseTrade(row);
    if (trade) out.push(trade);
  }
  return out;
}
