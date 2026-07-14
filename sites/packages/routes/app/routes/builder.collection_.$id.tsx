import { builderRedirect } from "@features/lib/creator/builder-redirect";

import type { Route } from "./+types/builder.collection_.$id";

export async function loader({ request, params }: Route.LoaderArgs) {
  return builderRedirect("collection-detail", request, params);
}
