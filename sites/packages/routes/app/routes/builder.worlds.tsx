import { redirect } from "react-router";

import { ensureSid, serializeSidCookie } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/builder.worlds";

const STORY = "creator-integration-redirect-worlds";
const FROM = "/builder/worlds";
const TO = "/creator-hub/manage";

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, created } = ensureSid(request);

  const query = new URL(request.url).searchParams.toString();
  const location = query ? `${TO}?${query}` : TO;

  track(
    "creator_builder_redirect",
    { from: FROM, to: TO, query },
    { sid, story: STORY },
  );

  return redirect(
    location,
    created ? { headers: { "Set-Cookie": serializeSidCookie(sid) } } : undefined,
  );
}
