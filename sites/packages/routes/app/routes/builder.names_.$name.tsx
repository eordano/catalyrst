import { redirect } from "react-router";

import type { Route } from "./+types/builder.names_.$name";

export async function loader(_: Route.LoaderArgs) {
  return redirect("/marketplace/names", 308);
}
