import { fetchCatalog, type FetchCatalogParams } from "./index";
import type { CatalogItem, MarketEnvelope } from "./schema";
import { ttlMemo } from "../../ttl-memo";

const RAIL_TTL_MS = 60_000;

const railMemo = ttlMemo({
  ttlMs: RAIL_TTL_MS,
  keyOf: (params: FetchCatalogParams) => JSON.stringify(params),
  load: (params): Promise<MarketEnvelope<CatalogItem[]>> => fetchCatalog(params),
});

// Visitor-independent catalog heads (shop overview rails, explore tab); one read per minute.
export function loadCatalogRail(
  params: FetchCatalogParams,
): Promise<MarketEnvelope<CatalogItem[]>> {
  return railMemo(params);
}

export function resetCatalogRailCache(): void {
  railMemo.reset();
}
