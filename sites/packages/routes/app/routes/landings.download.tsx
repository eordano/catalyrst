import { redirect } from "react-router";

import type { Route } from "./+types/landings.download";

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const params = new URLSearchParams();
  const position = url.searchParams.get("position");
  const realm = url.searchParams.get("realm");
  if (position) params.set("position", position);
  if (realm) params.set("realm", realm);
  const qs = params.toString();
  return redirect(`/play/${qs ? `?${qs}` : ""}`);
}

export default function LandingsDownloadRedirect() {
  return null;
}
