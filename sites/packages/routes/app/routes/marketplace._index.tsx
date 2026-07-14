import { redirect } from "react-router";

import { marketplaceMeta } from "@core/lib/seo/marketplace-meta";

import type { Route } from "./+types/marketplace._index";

export const meta = () => marketplaceMeta();

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  return redirect(`/shop${url.search}`);
}

export default function MarketplaceIndexRedirect() {
  return null;
}
