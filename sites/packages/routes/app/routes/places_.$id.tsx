import PlaceDetail from "@ui/explorer/pages/PlaceDetail";

import UpstreamUnavailable from "@features/components/UpstreamUnavailable";
import { CatalystError } from "@data/lib/catalyst/client";
import { type Place } from "@data/lib/catalyst/places/index";
import { loadPlace } from "@data/lib/catalyst/places/index.server";
import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { trackExposure } from "@core/lib/telemetry/track";
import JumpIn from "@features/stories/misc/jump-in/JumpIn";

import type { Route } from "./+types/places_.$id";

export const handle = { agentMarkdown: "placeDetail" } satisfies AgentMarkdownHandle;

const FALLBACK: Assignment = {
  variant: "control",
  flags: { confirmStep: false },
  experimentKey: "jump_in_confirm",
};

export async function loader({ request, params }: Route.LoaderArgs) {
  const { id } = params;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    "misc/jump-in",
    FALLBACK,
    { skipExposure: true },
  );

  let place: Place | null = null;
  let unavailable = false;
  try {
    place = await loadPlace(id, { signal: request.signal });
  } catch (err) {
    if (!(err instanceof CatalystError && err.status === 404)) {
      unavailable = true;
    }
  }

  trackExposure({
    sid,
    story: "jump-in",
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  });

  return wrap(
    { id, place, sid, assignment, unavailable },
    { status: place ? 200 : unavailable ? 503 : 404 },
  );
}

export default function PlaceDetailRoute({ loaderData }: Route.ComponentProps) {
  const { place, sid, assignment, unavailable } = loaderData;

  if (!place && unavailable) {
    return (
      <main className="place-detail-route">
        <UpstreamUnavailable
          title="Places are temporarily unavailable"
          message="We couldn't reach the places service. Please try again in a moment."
          backHref="/places"
          backLabel="Back to Places"
        />
      </main>
    );
  }

  return (
    <main className="place-detail-route">
      <PlaceDetail
        place={
          place
            ? {
                title: place.title ?? "",
                image: place.image ?? undefined,
                creator: place.contact_name ?? place.owner ?? "Decentraland",
                views: place.user_count ?? 0,
                approval: Math.round((place.like_rate ?? 0) * 100),
                description: place.description ?? "",
                coords: place.base_position ?? "",
                parcels: (place.positions ?? []).length,
                favorites: place.favorites ?? 0,
                updated: place.updated_at
                  ? new Date(place.updated_at).toLocaleDateString("en-GB")
                  : "",
                hue:
                  [...(place.id ?? "")].reduce((a, c) => a + c.charCodeAt(0), 0) % 360,
              }
            : undefined
        }
        notFound={!place}
      />

      {place && (
        <JumpIn
          place={{
            id: place.id,
            title: place.title ?? "",
            base_position: place.base_position,
            world: place.world,
            world_name: place.world_name,
          }}
          variant={assignment.variant}
          flags={assignment.flags}
          trackCtx={{
            sid,
            story: "jump-in",
            variant: assignment.variant,
            experimentKey: assignment.experimentKey,
          }}
        />
      )}
    </main>
  );
}
