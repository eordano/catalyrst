import type { z } from "zod";

import { catalystBase } from "../client";

import type { ItemQuoteOut as RsItemQuoteOut } from "@ui/generated/catalyst/credits/ItemQuoteOut";
import type { PriceQuotesOut as RsPriceQuotesOut } from "@ui/generated/catalyst/credits/PriceQuotesOut";
import {
  ItemQuoteOutSchema,
  PriceQuotesOutSchema,
} from "../generated-schemas/credits";

export const ItemQuoteSchema = ItemQuoteOutSchema;
export type ItemQuote = z.infer<typeof ItemQuoteSchema>;

export const PriceQuotesSchema = PriceQuotesOutSchema;
export type PriceQuotes = z.infer<typeof PriceQuotesSchema>;

export type QuoteRequest = {
  items?: { itemId: string; collection: string }[];
  amounts?: (string | null | undefined)[];
};

export async function quoteCreditPrices(
  req: QuoteRequest,
  opts: { base?: string; signal?: AbortSignal; fetchImpl?: typeof fetch } = {},
): Promise<PriceQuotes> {
  const body = {
    items: (req.items ?? []).map((r) => ({
      itemId: r.itemId,
      collection: r.collection,
    })),
    amounts: (req.amounts ?? []).map((a) => a ?? ""),
  };
  const doFetch = opts.fetchImpl ?? fetch;
  const res = await doFetch(`${catalystBase(opts.base)}/credits/prices/quote`, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify(body),
    signal: opts.signal,
    cache: "no-store",
  });
  if (!res.ok) throw new Error(`credits quote returned ${res.status}`);
  return PriceQuotesSchema.parse(await res.json());
}

export async function tryQuoteCreditPrices(
  req: QuoteRequest,
  opts: { base?: string; signal?: AbortSignal; fetchImpl?: typeof fetch } = {},
): Promise<PriceQuotes> {
  const empty: PriceQuotes = {
    items: (req.items ?? []).map((r) => ({
      itemId: r.itemId,
      collection: r.collection,
      credits: null,
    })),
    amounts: (req.amounts ?? []).map(() => null),
  };
  if (empty.items.length === 0 && empty.amounts.length === 0) return empty;
  try {
    return await quoteCreditPrices(req, opts);
  } catch {
    return empty;
  }
}

export const MAX_QUOTE_ITEMS = 60;

export type QuoteItemRef = { itemId: string; collection: string };

export async function tryQuoteCreditItems(
  refs: (QuoteItemRef | null)[],
  opts: { base?: string; signal?: AbortSignal; fetchImpl?: typeof fetch } = {},
): Promise<(string | null)[]> {
  const out: (string | null)[] = refs.map(() => null);
  const idx: number[] = [];
  const items: QuoteItemRef[] = [];
  refs.forEach((r, i) => {
    if (r) {
      idx.push(i);
      items.push({ itemId: r.itemId, collection: r.collection });
    }
  });
  if (items.length === 0) return out;

  const batches: Promise<PriceQuotes>[] = [];
  for (let start = 0; start < items.length; start += MAX_QUOTE_ITEMS) {
    batches.push(
      quoteCreditPrices(
        { items: items.slice(start, start + MAX_QUOTE_ITEMS) },
        opts,
      ).catch(
        (): PriceQuotes => ({
          items: items
            .slice(start, start + MAX_QUOTE_ITEMS)
            .map((r) => ({ ...r, credits: null })),
          amounts: [],
        }),
      ),
    );
  }
  const results = await Promise.all(batches);
  let cursor = 0;
  for (const res of results) {
    for (const q of res.items) {
      out[idx[cursor]] = q.credits ?? null;
      cursor += 1;
    }
  }
  return out;
}

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
type Assert<T extends true> = T;

export type _DriftItemQuote = Assert<Equal<ItemQuote, RsItemQuoteOut>>;
export type _DriftPriceQuotes = Assert<Equal<PriceQuotes, RsPriceQuotesOut>>;
