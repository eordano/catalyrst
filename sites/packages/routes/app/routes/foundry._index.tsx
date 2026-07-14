import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";

import EmptyState from "@ui/components/EmptyState";
import FoundryHome from "@ui/foundry/pages/FoundryHome";

import "@ui/components/emptystate.css";
import "@ui/foundry/pages/foundryhome.css";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";
import type { StoryId } from "@core/lib/telemetry/story-id";

import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import {
  homeSnapshot,
  programChecks,
  type ProgramCheck,
} from "@data/lib/foundry/program.server";

import type { Route } from "./+types/foundry._index";

const STORY: StoryId = "foundry/tour-activation";

const FALLBACK: Assignment = {
  variant: "control",
  flags: {},
  experimentKey: "foundry-tour-activation",
};

const EMPTY_STATS = {
  scenes: 0,
  gddDocs: 0,
  benchRuns: 0,
  lastBenchAt: null as string | null,
  copilotOnline: false,
  tokens: 0,
};

const MONTHS_FULL = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

export function meta() {
  return [{ title: "The Foundry" }];
}

// The "N games, deployed between X and Y" claim on the front door is derived from
// the loaded scene rows, never typed in: import an eighth game or let a creator
// redeploy and the sentence follows the data instead of becoming a lie.
function deriveDeploy(scenes: { deployedAt: string | null }[]): {
  count: number;
  range: string | null;
} {
  const times = scenes
    .map((s) => (s.deployedAt ? Date.parse(s.deployedAt) : NaN))
    .filter((t) => !Number.isNaN(t))
    .sort((a, b) => a - b);
  if (times.length === 0) return { count: 0, range: null };
  const lo = new Date(times[0]);
  const hi = new Date(times[times.length - 1]);
  const loM = lo.getUTCMonth();
  const loY = lo.getUTCFullYear();
  const hiM = hi.getUTCMonth();
  const hiY = hi.getUTCFullYear();
  let range: string;
  if (loY === hiY && loM === hiM) range = `${MONTHS_FULL[loM]} ${loY}`;
  else if (loY === hiY) range = `${MONTHS_FULL[loM]} and ${MONTHS_FULL[hiM]} ${loY}`;
  else range = `${MONTHS_FULL[loM]} ${loY} and ${MONTHS_FULL[hiM]} ${hiY}`;
  return { count: times.length, range };
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
  let checks: ProgramCheck[] = [];
  let deploy: { count: number; range: string | null } = { count: 0, range: null };
  let unavailable = false;

  try {
    const [snapshot, program] = await Promise.all([
      homeSnapshot(),
      programChecks(),
    ]);
    stats = {
      scenes: snapshot.scenes.length,
      gddDocs: snapshot.gddCount,
      benchRuns: snapshot.benchRuns,
      lastBenchAt: snapshot.lastBenchAt,
      copilotOnline: snapshot.copilotOnline,
      tokens: snapshot.llm.inputTokens + snapshot.llm.outputTokens,
    };
    checks = program;
    deploy = deriveDeploy(snapshot.scenes);
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({
    badge: sidBadge(sid),
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
    tourAutoStart: assignment.flags.tourAutoStart === true,
    unavailable,
    stats,
    checks,
    deploy,
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
  const [expandedIdea, setExpandedIdea] = useState<string | null>(null);

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

  if (d.unavailable) {
    return <FoundryUnavailable />;
  }

  function onIdeaToggle(id: string) {
    const next = expandedIdea === id ? null : id;
    setExpandedIdea(next);
    if (next) track("fd_idea_expanded", { idea_id: id }, ctx);
  }

  return (
    <FoundryHome
      stats={d.stats}
      checks={d.checks}
      deploy={d.deploy}
      expandedIdea={expandedIdea}
      onIdeaToggle={onIdeaToggle}
    />
  );
}

function FoundryUnavailable() {
  return (
    <EmptyState
      variant="inline"
      title="Foundry database not configured"
      subtitle="Program state lives in Postgres. Set FOUNDRY_DATABASE_URL on this deployment to read and write it."
    />
  );
}
