import { redirect } from "react-router";

import type { Route } from "./+types/explorer.map";

export async function loader(_args: Route.LoaderArgs) {
  return redirect("/explorer-map", 308);
}
