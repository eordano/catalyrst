import { builderRedirect } from "@features/lib/creator/builder-redirect";

import type { Route } from "./+types/builder.item_.$id";

export async function loader({ request, params }: Route.LoaderArgs) {
  return builderRedirect("item-detail", request, params);
}
