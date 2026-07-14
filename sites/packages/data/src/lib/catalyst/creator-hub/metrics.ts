import { z } from "zod";

export const ScenesSummarySchema = z.object({
  places: z.number().int().nonnegative(),
  visits30d: z.number().int().nonnegative(),
  liveNow: z.number().int().nonnegative(),
});
export type ScenesSummary = z.infer<typeof ScenesSummarySchema>;

export type SceneVisitRow = {
  title: string;
  location: string | null;
  href: string | null;
  visits30d: number | null;
  liveNow: number | null;
};

export const SummarySchema = z.object({
  publishedCollections: z.number().int().nonnegative().nullable(),
  onSaleItems: z.number().int().nonnegative().nullable(),
  sales7d: z.number().int().nonnegative().nullable(),
  salesVolumeMana7d: z.number().nonnegative().nullable(),
  // The flag that says the sales numbers above were not read. Defaulting it to
  // `false` claimed they had been -- exactly backwards -- so it is required and
  // `loadCreatorMetrics` is the only thing that decides it.
  salesUnavailable: z.boolean(),
  scenes: ScenesSummarySchema.nullable(),
});
export type Summary = z.infer<typeof SummarySchema>;

export function fmtCount(n: number): string {
  return n.toLocaleString("en-US");
}

export const DAY_MS = 86_400_000;

export function countPublished(
  collections: { status?: string | null; pending?: boolean }[],
): number {
  return collections.filter((c) => c.status && c.status !== "unsynced").length;
}

export function rollupSales(
  sales: { timestamp?: number | null; price?: string | null }[],
  now: number,
  windowDays = 7,
): { count: number; volumeMana: number } {
  const cutoff = now - windowDays * DAY_MS;
  let count = 0;
  let volumeWei = 0n;
  for (const s of sales) {
    const ts = s.timestamp ?? 0;
    if (ts < cutoff) continue;
    count += 1;
    try {
      volumeWei += BigInt(s.price ?? "0");
    } catch {
    }
  }
  const volumeMana = Number(volumeWei / 1_000000000000000000n);
  return { count, volumeMana };
}
