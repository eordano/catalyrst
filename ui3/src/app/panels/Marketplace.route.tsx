import MarketplaceFrame from "../../explorer/pages/MarketplaceFrame";
import type { QueryClient } from "@tanstack/react-query";
import { warmMarketplace } from "../../explorer/pages/marketplace-preload";

export function prefetch(_client: QueryClient, address?: string | null) {
  warmMarketplace(address ?? null);
}

export default function MarketplacePanel() {
  return <MarketplaceFrame />;
}
