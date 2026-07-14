import { useEffect, useRef } from "react";
import { useNavigate, useSearchParams } from "react-router";

import FoundryHome from "@ui/foundry/pages/FoundryHome";
import { FD_ROLES } from "@ui/foundry/components/FdDoors";
import type {
  FdRoleId,
  FdRoleStateLine,
} from "@ui/foundry/components/FdRoleCard";
import { plural } from "@ui/foundry/fmt";

import "@ui/atoms/button.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdrolecard.css";
import "@ui/foundry/pages/foundryhome.css";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import type { StoryId } from "@core/lib/telemetry/story-id";

import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import { homeSnapshot } from "@data/lib/foundry/program.server";

import type { Route } from "./+types/foundry._index";

const STORY: StoryId = "foundry/tour-activation";

const FALLBACK: Assignment = {
  variant: "control",
  flags: {},
  experimentKey: "foundry-tour-activation",
};

// Keeps fd_home_viewed's shape when the program database is unreadable; the
// doors then render static, with no state line claiming a reading.
const EMPTY_STATS = {
  scenes: 0,
  gddDocs: 0,
  benchRuns: 0,
  copilotOnline: false,
};

export function meta() {
  return [{ title: "The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  // A tour param can only be here after a clean front-door load already fired
  // exposure — both the auto-start navigate and the tour button add it. Counting
  // it again would double the auto arm's denominator against control's.
  const url = new URL(request.url);
  const inTour = url.searchParams.has("tour") || url.searchParams.has("tour_src");
  const { sid, assignment, wrap } = await storyLoader(request, STORY, FALLBACK, {
    skipExposure: inTour,
  });

  let stats = EMPTY_STATS;
  let doorState: Partial<Record<FdRoleId, FdRoleStateLine>> | null = null;

  try {
    const snapshot = await homeSnapshot();
    stats = {
      scenes: snapshot.scenes.length,
      gddDocs: snapshot.gddCount,
      benchRuns: snapshot.benchRuns,
      copilotOnline: snapshot.copilotOnline,
    };
    const live = snapshot.scenes.filter((s) => s.entityId !== null).length;
    doorState = {
      start: {
        text: `${plural(live, "game")} live`,
        href: "/foundry/play",
        title: "games carrying a Worlds deployment entity",
      },
      create: {
        text: plural(snapshot.requests, "open ask"),
        href: "/foundry/exchange",
        title: "requests on the exchange that are not closed",
      },
      admin: {
        text: `${plural(snapshot.benchRuns, "run")} recorded`,
        href: "/foundry/console/bench",
        title: "bot runs stored by the bench harness",
      },
    };
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
  }

  return wrap({
    badge: sidBadge(sid),
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
    tourAutoStart: assignment.flags.tourAutoStart === true,
    doorState,
    stats,
  });
}

export default function FoundryFrontDoor({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = {
    sid: d.badge,
    story: STORY,
    variant: d.variant,
    experimentKey: d.experimentKey,
  };
  const navigate = useNavigate();
  const [params] = useSearchParams();

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track(
      "fd_home_viewed",
      {
        bench_runs: d.stats.benchRuns,
        copilot_online: d.stats.copilotOnline,
        gdd_total: d.stats.gddDocs,
        scenes_total: d.stats.scenes,
      },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  const autoStarted = useRef(false);
  useEffect(() => {
    if (autoStarted.current) return;
    autoStarted.current = true;
    if (!d.tourAutoStart) return;
    if (params.get("tour")) return;
    const latch = `fd_tour_auto:${d.experimentKey}`;
    try {
      if (localStorage.getItem(latch)) return;
      localStorage.setItem(latch, "1");
    } catch {
      return;
    }
    const search = new URLSearchParams({ tour: "1", tour_src: "auto" });
    const variantParam = params.get("variant");
    if (variantParam) search.set("variant", variantParam);
    navigate(`/foundry?${search.toString()}`, { replace: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.tourAutoStart, d.experimentKey]);

  function onChoose(id: FdRoleId) {
    const door = FD_ROLES.find((r) => r.id === id);
    if (!door) return;
    track("fd_role_chosen", { role: id, destination: door.destinationId }, ctx);
  }

  return <FoundryHome onChoose={onChoose} doorState={d.doorState ?? undefined} />;
}
