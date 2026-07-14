import { useEffect, useRef } from "react";
import { data } from "react-router";

import type {
  FdEmotionalJobLetter,
  FdEmotionalJobVM,
} from "@ui/foundry/components/FdEmotionalJobs";
import {
  MARKET_CELL_NAMES,
  type FdMarketCellSlug,
} from "@ui/foundry/components/FdGameCard";
import FdResponse, {
  type FdResponseAskAnswerVM,
  type FdResponseGatheringVM,
  type FdResponseMemoryVM,
  type FdResponseRevisionVM,
  type FdResponseRunVM,
  type FdResponseSignalsVM,
} from "@ui/foundry/pages/FdResponse";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdcellchip.css";
import "@ui/foundry/pages/fdresponse.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { listBenchReports } from "@data/lib/foundry/bench.server";
import { listSceneMemory } from "@data/lib/foundry/continuity.server";
import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { listSceneJobs, listServedJobs } from "@data/lib/foundry/emotional-jobs.server";
import { listShelfAnswerAsks } from "@data/lib/foundry/request-readings.server";
import {
  MEASURED_FLOOR,
  MEASURED_SINCE_LABEL,
  ResponseSignalsUnavailableError,
  getResponseSignalsPool,
  isResponseSignalsConfigured,
  readResponseSignals,
  reportLine,
  revisionSplit,
} from "@data/lib/foundry/response.server";
import { getScene, listScenes } from "@data/lib/foundry/scenes.server";
import { listUpcoming } from "@data/lib/foundry/sessions.server";
import { listTrajectories } from "@data/lib/foundry/trajectory.server";

import type { Route } from "./+types/foundry.play_.$slug_.response";

// The readings page: one shareable per-game read of everything this program has
// measured about the game — the deck's "show who returned, what mattered and
// what changed after revision", scoped to exactly what is measured today. Every
// number links to where it comes from, and a signal nobody measured renders as
// its stated absence, never as a zero. A player's way to respond is the
// exchange's ask+pledge flow, which the page points to.

export function meta({ loaderData }: Route.MetaArgs) {
  const title = (loaderData as { title?: string | null } | undefined)?.title;
  return [
    { title: title ? `Readings — ${title} — The Foundry` : "Readings — The Foundry" },
  ];
}

const ALL_JOBS: FdEmotionalJobLetter[] = ["A", "B", "C", "D", "E", "F"];

export async function loader({ params, request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const slug = params.slug;

  try {
    const db = getPool();
    const scene = await getScene(db, slug);
    if (!scene) throw data(null, { status: 404 });

    const [
      reports,
      trajectories,
      memory,
      jobs,
      scenes,
      servedJobs,
      occurrences,
      askAnswers,
    ] = await Promise.all([
      listBenchReports(db, scene.id),
      listTrajectories(db, { sceneId: scene.id }),
      listSceneMemory(scene.id),
      listSceneJobs(db, scene.id),
      listScenes(db),
      listServedJobs(db),
      listUpcoming(sid, scene.id),
      listShelfAnswerAsks(scene.id, db),
    ]);

    let signals: FdResponseSignalsVM | null = null;
    let signalsUnreadable = false;
    if (isResponseSignalsConfigured()) {
      try {
        const s = await readResponseSignals(getResponseSignalsPool(), {
          slug: scene.id,
          trajectoryIds: trajectories.map((t) => t.id),
        });
        signals = {
          visits: {
            days: s.visitDays,
            totalEvents: s.visitEvents,
            distinctVisitors: s.distinctVisitors,
          },
          replays: s.replays.map((r) => {
            const report = reports.find((b) => b.trajectoryId === r.trajectoryId);
            const trajectory = trajectories.find((t) => t.id === r.trajectoryId);
            return {
              ...r,
              ranAt: report?.ranAt ?? trajectory?.createdAt ?? null,
              sandbox:
                (report?.runner ?? trajectory?.runner) === "arena",
            };
          }),
          downloads: s.downloads,
        };
      } catch (err) {
        if (!(err instanceof ResponseSignalsUnavailableError)) throw err;
        // Configured but unreadable — a different honest sentence than
        // "not connected yet".
        signalsUnreadable = true;
      }
    }

    // Program-level gaps, derived from the registry's own rows — the creator's
    // what-to-build-next pointer. null = the registry was never read at all,
    // which is a different absence from "read, and every slot is served".
    const cellsRead = scenes.some((s) => s.marketCell !== null);
    const cellGaps: FdMarketCellSlug[] | null = cellsRead
      ? (Object.keys(MARKET_CELL_NAMES) as FdMarketCellSlug[]).filter(
          (c) => !scenes.some((s) => s.marketCell?.cell === c),
        )
      : null;
    const jobGaps: FdEmotionalJobLetter[] | null = servedJobs.anyRead
      ? ALL_JOBS.filter((j) => !servedJobs.served.includes(j))
      : null;

    // The game's changelog only — the stewardship actions live on the game
    // page's continuity section. A row a visitor authored carries a name or a
    // badge actor; an imported row carries its source.
    const changelog = memory.filter((m) => m.action === "changelog");
    const hasVisitorNote = changelog.some((m) => !("source" in m.actor));

    const split = revisionSplit(scene.deployedAt, signals?.visits.days ?? []);
    const deployedDay = scene.deployedAt ? scene.deployedAt.slice(0, 10) : null;
    const revision: FdResponseRevisionVM = split
      ? { kind: "split", ...split }
      : deployedDay && deployedDay >= MEASURED_FLOOR
        ? { kind: "thin", deployedDay }
        : { kind: "none" };

    return wrap({
      badge: sidBadge(sid),
      unavailable: false,
      slug,
      title: scene.title,
      gameHref: `/foundry/play/${encodeURIComponent(scene.id)}`,
      measuredSince: MEASURED_SINCE_LABEL,
      signals,
      signalsUnreadable,
      gatherings: occurrences.map((o) => ({
        seriesId: o.seriesId,
        title: o.title,
        occurrenceAt: o.occurrenceAt,
        rsvpCount: o.rsvpCount,
      })) satisfies FdResponseGatheringVM[],
      runs: reports.map(reportLine) satisfies FdResponseRunVM[],
      marketCell: scene.marketCell,
      emotionalJobs: jobs satisfies FdEmotionalJobVM[],
      cellGaps,
      jobGaps,
      askAnswers: askAnswers satisfies FdResponseAskAnswerVM[],
      gddHref: scene.gddDocId ? `/foundry/gdd/${scene.gddDocId}` : null,
      memory: changelog.map((m) => ({
        eventId: m.eventId,
        at: m.at,
        body: m.body,
        sourceNote: m.sourceNote,
      })) satisfies FdResponseMemoryVM[],
      hasVisitorNote,
      revision,
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    return wrap({
      badge: sidBadge(sid),
      unavailable: true,
      slug,
      title: null,
      gameHref: `/foundry/play/${encodeURIComponent(slug)}`,
      measuredSince: MEASURED_SINCE_LABEL,
      signals: null,
      gatherings: [] as FdResponseGatheringVM[],
      runs: [] as FdResponseRunVM[],
      marketCell: null,
      emotionalJobs: null,
      cellGaps: null,
      jobGaps: null,
      askAnswers: [] as FdResponseAskAnswerVM[],
      gddHref: null,
      memory: [] as FdResponseMemoryVM[],
      hasVisitorNote: false,
      revision: { kind: "none" } as FdResponseRevisionVM,
    });
  }
}

export default function FoundryGameResponse({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || d.unavailable) return;
    viewed.current = true;
    track("fd_response_viewed", { slug: d.slug }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge, d.slug]);

  if (d.unavailable || d.title === null) {
    return (
      <div className="fd-page fd-stack">
        <FdPageHead
          eyebrow="Readings"
          title="Game"
          crumbs={<a href="/foundry/play">← All games</a>}
        />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return (
    <FdResponse
      title={d.title}
      slug={d.slug}
      gameHref={d.gameHref}
      measuredSince={d.measuredSince}
      signals={d.signals}
      signalsUnreadable={d.signalsUnreadable === true}
      gatherings={d.gatherings}
      runs={d.runs}
      marketCell={d.marketCell}
      emotionalJobs={d.emotionalJobs}
      cellGaps={d.cellGaps}
      jobGaps={d.jobGaps}
      askAnswers={d.askAnswers}
      gddHref={d.gddHref}
      memory={d.memory}
      hasVisitorNote={d.hasVisitorNote}
      revision={d.revision}
    />
  );
}
