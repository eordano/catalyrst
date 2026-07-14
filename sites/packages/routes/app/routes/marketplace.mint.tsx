import { redirect } from "react-router";

import type { Route } from "./+types/marketplace.mint";

export async function loader({ request }: Route.LoaderArgs) {
  const itemId = new URL(request.url).searchParams.get("item")?.trim();
  return redirect(
    itemId ? `/marketplace/${encodeURIComponent(itemId)}` : "/shop",
    308,
  );
}
