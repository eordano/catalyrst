import { redirect } from "react-router";

import { marketplaceMeta } from "@core/lib/seo/marketplace-meta";

import type { Route } from "./+types/marketplace.browse";

export const meta = () => marketplaceMeta("Browse");

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  return redirect(`/shop${url.search}`);
}

export default function MarketplaceBrowseRedirect() {
  return null;
}
