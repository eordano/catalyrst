import { redirect } from "react-router";

import { ensureSid, serializeSidCookie } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/creator-hub._index";

const FROM = "/creator-hub";
const TO = "/create";

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, created } = ensureSid(request);

  track("creator_builder_redirect", { from: FROM, to: TO }, { sid });

  return redirect(
    TO,
    created ? { headers: { "Set-Cookie": serializeSidCookie(sid) } } : undefined,
  );
}

export default function CreatorHubIndexRedirect() {
  return null;
}
