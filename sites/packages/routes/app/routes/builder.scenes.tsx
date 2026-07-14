import { redirect } from "react-router";

import { ensureSid, serializeSidCookie } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/builder.scenes";

const STORY = "creator-integration-redirect-scenes";
const FROM = "/builder/scenes";
const TO = "/create/scenes";

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, created } = ensureSid(request);

  const params = new URL(request.url).searchParams;
  const address = params.get("address");
  if (address !== null) {
    params.delete("address");
    params.set("creator", address);
  }
  const query = params.toString();
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
