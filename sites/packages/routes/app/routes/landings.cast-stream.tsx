import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import CastConsole from "@features/stories/landings/cast-stream/CastConsole";

import type { Route } from "./+types/landings.cast-stream";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/cast-stream";

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "console",
  flags: { guidedConsole: true },
  experimentKey: "st_cast_console",
};

const DEFAULT_TOKEN = "demo-stream-key";

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const tokenParam = url.searchParams.get("token");
  const token = tokenParam === null ? DEFAULT_TOKEN : tokenParam;
  const identity = url.searchParams.get("name")?.trim() || undefined;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    DEFAULT_ASSIGNMENT,
  );

  const payload = { sid, step, token, identity: identity ?? null, assignment };
  return wrap(payload);
}

export default function LandingsCastStream({ loaderData }: Route.ComponentProps) {
  const { sid, step, token, identity, assignment } = loaderData;

  return (
    <main className="cast-stream">
      <CastConsole
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        token={token}
        identity={identity ?? undefined}
        initialStep={step ?? undefined}
      />
    </main>
  );
}
