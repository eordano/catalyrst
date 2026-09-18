import { fetchCatalog, type FetchCatalogParams } from "./index";
import type { CatalogItem, MarketEnvelope } from "./schema";
import { publicFeedMemo } from "../../public-feed.server";
import type { PublicFreshness } from "@ui/data/screens/section";
import { withDeadline } from "../../request-deadline";

const RAIL_TTL_MS = 60_000;

const railMemo = publicFeedMemo({
  freshMs: RAIL_TTL_MS,
  keyOf: (params: FetchCatalogParams) => JSON.stringify(params),
  load: (params): Promise<MarketEnvelope<CatalogItem[]>> =>
    withDeadline((signal) => fetchCatalog(params, { signal }), 3_000),
});

// Visitor-independent catalog heads (shop overview rails, explore tab); one read per minute.
export async function loadCatalogRail(
  params: FetchCatalogParams,
): Promise<MarketEnvelope<CatalogItem[]> & { freshness: PublicFreshness }> {
  const { data, ...freshness } = await railMemo(params);
  return { ...data, freshness };
}

export function resetCatalogRailCache(): void {
  railMemo.reset();
}
