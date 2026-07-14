import Minimap from "@ui/explorer/frames/Minimap";

import { loadMapJump } from "@data/lib/catalyst/overlay/map-jump.server";
import {
  findPinByCoords,
  parseCoords,
  type MapJumpData,
} from "@data/lib/catalyst/overlay/map-jump";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import type { Route } from "./+types/bevy-overlay.minimap";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "client/hud-overlay";

const FALLBACK: Assignment = {
  variant: "overlay",
  flags: { domOverlay: true },
  experimentKey: "client_hud_overlay",
};

function resolveCoords(raw: string | null, map: MapJumpData): string {
  if (raw) {
    const [x, y] = parseCoords(raw);
    return `${x},${y}`;
  }
  return map.pins[0]?.coords ?? "0,0";
}

function parseHeading(raw: string | null): number | null {
  if (raw === null) return null;
  const n = Number.parseFloat(raw);
  return Number.isFinite(n) ? n : null;
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const coordsParam = url.searchParams.get("coords")?.trim() || null;
  const heading = parseHeading(url.searchParams.get("heading"));

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const map = await loadMapJump({ signal: request.signal });
  const coords = resolveCoords(coordsParam, map);
  const place = findPinByCoords(map.pins, coords)?.name ?? "";

  const payload = {
    sid,
    coords,
    place,
    heading,
    assignment,
  };
  return wrap(payload);
}

export default function BevyOverlayMinimap({ loaderData }: Route.ComponentProps) {
  const { place, coords, heading } = loaderData;

  return (
    <main className="bevy-overlay-minimap">
      <Minimap place={place} coords={coords} heading={heading ?? undefined} />
    </main>
  );
}
