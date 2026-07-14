import { redirect } from "react-router";

import { buildRedirect } from "@data/lib/catalyst/creator-hub/integration-redirect-collections";
import { ensureSid, serializeSidCookie } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/builder.collections";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "creator-hub/integration-redirect-collections";
const FROM = "/builder/collections";

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, created } = ensureSid(request);

  const { location, to, query } = buildRedirect(request.url);

  track(
    "creator_builder_redirect",
    { from: FROM, to, query },
    { sid, story: STORY },
  );

  return redirect(
    location,
    created ? { headers: { "Set-Cookie": serializeSidCookie(sid) } } : undefined,
  );
}
