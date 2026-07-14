import { redirect } from "react-router";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import { isWorldLocation, type WatchResult } from "@data/lib/catalyst/landings/cast-watcher";
import {
  resolveWatch,
  fallbackPlaceName,
  type WatchIntent,
} from "@data/lib/catalyst/landings/cast-watcher.server";
import CastWatch from "@features/stories/landings/cast-watch/CastWatch";

import type { Route } from "./+types/landings.cast-watch";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "landings/cast-watch";

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "watcher",
  flags: { guidedWatch: true },
  experimentKey: "st_cast_watch",
};

const DEFAULT_LOCATION = "0,0";
const DEFAULT_IDENTITY = "Viewer";

const DEMO_PARTICIPANT_COUNT = 12;
const DEMO_UNREAD_COUNT = 3;

function parseIntent(raw: string | null): WatchIntent {
  const v = raw?.trim().toLowerCase();
  if (v === "0" || v === "waiting" || v === "false") return "waiting";
  if (v === "expired") return "expired";
  return "live";
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const rawLocation = url.searchParams.get("location");
  if (rawLocation !== null && rawLocation.trim() === "") {
    throw redirect("/landings/cast-not-found?from=watcher&reason=missing");
  }

  const location = rawLocation?.trim() || DEFAULT_LOCATION;
  const identity = url.searchParams.get("name")?.trim() || DEFAULT_IDENTITY;
  const intent = parseIntent(url.searchParams.get("active"));

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    DEFAULT_ASSIGNMENT,
  );

  const trackCtx = {
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  };

  const watch: WatchResult = await resolveWatch(location, identity, intent).catch(
    (): WatchResult => ({
      status: intent === "live" ? "waiting" : intent,
      location,
      isWorld: isWorldLocation(location),
      placeName: fallbackPlaceName(location),
      identity,
      error: intent === "expired" ? "access_expired" : "no_active_stream",
      fallback: true,
    }),
  );

  track("cast_watch_opened", { location, is_world: watch.isWorld }, trackCtx);

  if (watch.status === "waiting") {
    track("cast_watch_no_stream", { location }, trackCtx);
  } else if (watch.status === "expired") {
    track("cast_watch_access_expired", { location }, trackCtx);
  }

  const payload = { sid, watch, assignment };
  return wrap(payload);
}

export default function LandingsCastWatch({ loaderData }: Route.ComponentProps) {
  const { sid, watch, assignment } = loaderData;

  const trackCtx = {
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  };

  return (
    <main className="cast-watch-route">
      <CastWatch
        trackCtx={trackCtx}
        watch={watch}
        participantCount={DEMO_PARTICIPANT_COUNT}
        unreadCount={DEMO_UNREAD_COUNT}
      />
    </main>
  );
}
