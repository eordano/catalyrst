import { redirect } from "react-router";

import type { Route } from "./+types/creator-hub.deploy-alternative";

export async function loader(_args: Route.LoaderArgs) {
  return redirect("/creator-hub/deploy-world", 308);
}
