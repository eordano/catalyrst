import { redirect } from "react-router";

import { ensureSid, serializeSidCookie } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/create.collections";

const FROM = "/create/collections";
const TO = "/create/wearables";

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, created } = ensureSid(request);

  track("creator_builder_redirect", { from: FROM, to: TO }, { sid });

  const to = `${TO}${new URL(request.url).search}`;
  return redirect(
    to,
    created ? { headers: { "Set-Cookie": serializeSidCookie(sid) } } : undefined,
  );
}

export default function CreateCollectionsRedirect() {
  return null;
}
