import { redirect } from "react-router";

import type { Route } from "./+types/builder.land_.$id";

export async function loader({ params }: Route.LoaderArgs) {
  return redirect("/shop", 308);
}
