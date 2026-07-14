import { redirect } from "react-router";

import type { Route } from "./+types/builder.collections_.$id";

export async function loader({ params }: Route.LoaderArgs) {
  return redirect(`/create/wearables/collections/${params.id}`, 308);
}
