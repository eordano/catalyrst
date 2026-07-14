import { redirect } from "react-router";

import { marketplaceMeta } from "@core/lib/seo/marketplace-meta";

import type { Route } from "./+types/marketplace.shop";

export const meta = () => marketplaceMeta("Shop");

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  return redirect(`/shop${url.search}`);
}

export default function MarketplaceShopRedirect() {
  return null;
}
