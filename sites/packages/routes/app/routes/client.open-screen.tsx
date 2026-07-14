import { redirect } from "react-router";

import { fetchMostActivePlaces, type Place } from "@data/lib/catalyst/places/index";
import { loadPlaces } from "@data/lib/catalyst/places/index.server";
import {
  selectLiveTargets,
  type OpenPlace,
} from "@features/stories/client/open-screen/select";
import { type Assignment } from "@core/lib/experiments/assign";
import { experimentActive } from "@core/lib/experiments/flags";
import { parseVariantOverride, storyLoader } from "@core/lib/experiments/story-loader";
import {
  OPEN_SCREEN_ARMS,
  OPEN_SCREEN_EXPERIMENT_KEY,
  OPEN_SCREEN_TARGETS,
  activeOpenScreenExperiment,
  armOverride,
  openScreenFromFlags,
  type OpenScreenArm,
} from "@core/lib/experiments/open-screen";
import { trackExposure } from "@core/lib/telemetry/track";
import OpenScreen from "@features/stories/client/open-screen/OpenScreen";

import type { Route } from "./+types/client.open-screen";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "client/open-screen";

const BROWSE_LIMIT = 24;
const ACTIVE_LIMIT = 30;

const FALLBACK: Assignment = {
  variant: "base",
  flags: { openScreen: "base" },
  experimentKey: OPEN_SCREEN_EXPERIMENT_KEY,
};

function forcedArm(url: URL): OpenScreenArm | undefined {
  const raw =
    armOverride(url, OPEN_SCREEN_ARMS.map((id) => ({ id }))) ??
    parseVariantOverride(url, OPEN_SCREEN_EXPERIMENT_KEY);
  if (!raw) return undefined;
  return (OPEN_SCREEN_ARMS as readonly string[]).includes(raw)
    ? (raw as OpenScreenArm)
    : undefined;
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);

  const { sid, userKey, assignment: resolved, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
    { skipExposure: true },
  );

  // Draft until activated: a non-empty flags-service override row (or the
  // OPEN_SCREEN_EXPERIMENT env var) turns it on; otherwise every session gets
  // base. ?arm= / ?variant=client_open_screen:<arm> still force a preview.
  const active = await experimentActive(OPEN_SCREEN_EXPERIMENT_KEY, {
    envActive:
      activeOpenScreenExperiment(
        typeof process !== "undefined" ? process.env?.OPEN_SCREEN_EXPERIMENT : undefined,
      ) !== null,
    user: userKey,
  });
  let assignment = active ? resolved : FALLBACK;
  const forced = forcedArm(url);
  if (forced) {
    assignment = {
      variant: forced,
      flags: { openScreen: forced },
      experimentKey: OPEN_SCREEN_EXPERIMENT_KEY,
    };
  }

  const arm: OpenScreenArm = openScreenFromFlags(assignment.flags) ?? "base";

  let places: Place[] | null = null;
  let busiest: OpenPlace | null = null;
  let surprise: OpenPlace | null = null;

  if (arm === "base") {
    places = await loadPlaces({ limit: BROWSE_LIMIT })
      .then((r) => r.data)
      .catch(() => null);
  } else {
    const mostActive = await fetchMostActivePlaces(
      { limit: ACTIVE_LIMIT },
      { signal: request.signal },
    ).catch(() => null);
    ({ busiest, surprise } = selectLiveTargets(mostActive));
  }

  // No live scene reading for the genesis arm: never park the player on a
  // dead-end "browse instead" prompt -- send them straight to the Places grid,
  // server-side, so there is no screen and no click. This runs BEFORE
  // trackExposure so a session that can never see the genesis screen is not
  // counted as a genesis exposure, keeping the arm's conversion rate honest.
  if (arm === "genesis" && !busiest) {
    throw redirect(OPEN_SCREEN_TARGETS.explore);
  }

  // A forced arm is QA/preview driving the surface, and an inactive experiment
  // samples nobody: neither counts as an exposure.
  if (active && !forced) {
    trackExposure({
      sid,
      story: STORY,
      variant: assignment.variant,
      experimentKey: assignment.experimentKey,
    });
  }

  return wrap({ sid, assignment, arm, places, busiest, surprise });
}

export default function ClientOpenScreen({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  return (
    <main className="client-open-screen-route">
      <OpenScreen
        arm={d.arm}
        places={d.places}
        busiest={d.busiest}
        surprise={d.surprise}
        trackCtx={{
          sid: d.sid,
          story: STORY,
          variant: d.assignment.variant,
          experimentKey: d.assignment.experimentKey,
        }}
      />
    </main>
  );
}
