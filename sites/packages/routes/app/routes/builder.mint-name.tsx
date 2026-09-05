import { redirect } from "react-router";

import type { Route } from "./+types/builder.mint-name";

export async function loader({ params }: Route.LoaderArgs) {
  return redirect("/marketplace/names", 308);
}
