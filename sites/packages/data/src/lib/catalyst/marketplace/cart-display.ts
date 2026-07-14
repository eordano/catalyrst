import { useEffect, useMemo, useState } from "react";

import { fetchCatalogItem } from "./index";
import { thumbnailFromUrn } from "./account";
import type { CartLine } from "./cart";

export type DisplayLine = {
  key: string;
  name: string;
  thumbnail?: string;
  qty: number;
  unitPriceCredits: string;
};

export function fallbackDisplayLine(l: CartLine): DisplayLine {
  return {
    key: `${l.collection ?? ""}-${l.itemId}`,
    name: l.category || l.itemId || "Item",
    thumbnail: thumbnailFromUrn(l.urn),
    qty: l.qty,
    unitPriceCredits: l.unitPriceCredits,
  };
}

export async function resolveDisplayLine(
  l: CartLine,
  signal?: AbortSignal,
): Promise<DisplayLine> {
  const fb = fallbackDisplayLine(l);
  if (!l.collection) return fb;
  const item = await fetchCatalogItem(l.collection, l.itemId, { signal }).catch(
    () => null,
  );
  if (!item) return fb;
  return {
    ...fb,
    name: item.name ?? fb.name,
    thumbnail: item.thumbnail ?? fb.thumbnail,
  };
}

export function useDisplayLines(source: CartLine[]): DisplayLine[] {
  const fallback = useMemo(() => source.map(fallbackDisplayLine), [source]);
  const [rows, setRows] = useState<DisplayLine[]>(fallback);

  useEffect(() => {
    setRows(fallback);
    if (source.length === 0) return;
    const controller = new AbortController();
    let cancelled = false;
    Promise.all(source.map((l) => resolveDisplayLine(l, controller.signal)))
      .then((resolved) => {
        if (!cancelled) setRows(resolved);
      })
      // Nothing to hand back when the enrichment pass fails: the rows stay at
      // `fallback`, built from the cart lines themselves, so no name, quantity
      // or price is invented for a lookup that did not answer.
      .catch(() => undefined);
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [source, fallback]);

  return rows;
}
