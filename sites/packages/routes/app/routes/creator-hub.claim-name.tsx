import { redirect } from "react-router";

import { ensureSid, serializeSidCookie } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/creator-hub.claim-name";

const FROM = "/creator-hub/claim-name";

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, created } = ensureSid(request);

  const target = `/marketplace/claim-name${new URL(request.url).search}`;

  track("creator_claim_name_redirect", { from: FROM, to: target }, { sid });

  return redirect(
    target,
    created ? { headers: { "Set-Cookie": serializeSidCookie(sid) } } : undefined,
  );
}

export default function CreatorHubClaimName() {
  return null;
}
