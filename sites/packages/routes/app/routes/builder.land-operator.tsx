import { redirect } from "react-router";

import type { Route } from "./+types/builder.land-operator";

export async function loader(_: Route.LoaderArgs) {
  return redirect("/shop", 308);
}
