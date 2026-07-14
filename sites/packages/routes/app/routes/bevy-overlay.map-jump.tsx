import { loadMapJump } from "@data/lib/catalyst/overlay/map-jump.server";
import {
  unavailableMapJump,
  type MapJumpData,
} from "@data/lib/catalyst/overlay/map-jump";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import MapJumpWizard from "@features/stories/overlay/map-jump/MapJumpWizard";

import type { Route } from "./+types/bevy-overlay.map-jump";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/map-jump";

const FALLBACK: Assignment = {
  variant: "navmap",
  flags: { navmap: true, confirmStep: true },
  experimentKey: "cl_map_jump",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const filter = url.searchParams.get("filter")?.trim() || null;
  const select = url.searchParams.get("select")?.trim() || null;
  const step = url.searchParams.get("step")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let mapData: MapJumpData;
  try {
    mapData = await loadMapJump({ signal: request.signal });
  } catch (err) {
    mapData = unavailableMapJump((err as Error)?.message ?? "network error");
  }

  const payload = {
    sid,
    filter,
    select,
    step,
    assignment,
    map: mapData,
  };
  return wrap(payload);
}

export default function BevyOverlayMapJump({ loaderData }: Route.ComponentProps) {
  const { sid, filter, select, step, assignment, map } = loaderData;

  return (
    <main className="bevy-overlay-map-jump">
      <MapJumpWizard
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        data={map}
        initialFilter={filter ?? undefined}
        initialSelect={select ?? undefined}
        initialStep={step ?? undefined}
      />
    </main>
  );
}
