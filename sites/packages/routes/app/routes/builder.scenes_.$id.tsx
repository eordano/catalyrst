import { redirect } from "react-router";

import type { Route } from "./+types/builder.scenes_.$id";

export async function loader({ params }: Route.LoaderArgs) {
  return redirect("/create/scenes", 308);
}
