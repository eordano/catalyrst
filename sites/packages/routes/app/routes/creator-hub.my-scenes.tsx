import { redirect } from "react-router";

import type { Route } from "./+types/creator-hub.my-scenes";

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const params = new URLSearchParams(url.search);
  params.set("view", "published");
  return redirect(`/create/scenes?${params.toString()}`);
}

export default function CreatorHubMyScenes() {
  return null;
}
