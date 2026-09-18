import { redirect } from "react-router";

import type { Route } from "./+types/builder.scene-editor";

export async function loader(_: Route.LoaderArgs) {
  return redirect("/create/scenes", 308);
}
