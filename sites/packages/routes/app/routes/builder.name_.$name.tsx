import { redirect } from "react-router";

import type { Route } from "./+types/builder.name_.$name";

export async function loader({ params }: Route.LoaderArgs) {
  return redirect("/marketplace/names", 308);
}
