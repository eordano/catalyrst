import { redirect } from "react-router";

import type { Route } from "./+types/foundry.console._index";

export async function loader({ request }: Route.LoaderArgs) {
  const { search } = new URL(request.url);
  return redirect(`/foundry/console/bench${search}`);
}
