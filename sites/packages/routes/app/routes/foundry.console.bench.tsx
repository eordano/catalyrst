import { useEffect, useRef } from "react";

import { evidenceLabel } from "@ui/foundry/components/evidence";
import FdBenchPage, {
  type FdBenchReportVM,
  type FdBenchTargetVM,
} from "@ui/foundry/pages/FdBenchPage";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdstat.css";
import "@ui/foundry/components/fdverdictpill.css";
import "@ui/foundry/pages/fdbench.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import {
  benchTargets,
  countBenchReports,
  listBenchReports,
} from "@data/lib/foundry/bench.server";
import { reportShotHrefs } from "@data/lib/foundry/evidence.server";
import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { listScenes } from "@data/lib/foundry/scenes.server";
import type { BotReport } from "@data/lib/foundry/types";

import type { Route } from "./+types/foundry.console.bench";

export function meta() {
  return [{ title: "Runs — The Foundry" }];
}

function toReportVM(
  report: BotReport,
  titles: ReadonlyMap<string, string>,
  shotHrefs: string[],
): FdBenchReportVM {
  return {
    shotHrefs,
    id: report.id,
    slug: report.slug,
    title: report.sceneId ? titles.get(report.sceneId) ?? null : null,
    runner: report.runner,
    realm: report.realm,
    ranAt: report.ranAt,
    verdict: report.verdict,
    checksTotal: report.checksTotal,
    checksFailed: report.checksFailed,
    checksUnevaluable: report.checksUnevaluable,
    missingTools: report.missingTools,
    stubbedTools: report.stubbedTools,
    networkWrites: report.networkWrites,
    shots: report.shots.length,
    // Label only — the raw absolute evidence path never leaves the server.
    evidence: report.evidencePath ? evidenceLabel(report.evidencePath) : null,
    evidenceHref: report.evidencePath
      ? `/foundry/console/evidence/${report.id}`
      : null,
    replayHref: report.trajectoryId
      ? `/foundry/console/trajectories/${report.trajectoryId}`
      : null,
    gameHref: report.sceneId ? `/foundry/play/${report.sceneId}` : null,
  };
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let reports: FdBenchReportVM[] = [];
  let reportsTotal = 0;
  let sandboxTotal = 0;
  let targets: FdBenchTargetVM[] | null = null;
  let unavailable = false;

  try {
    const db = getPool();
    const [rows, counts, scenes, targetRows] = await Promise.all([
      listBenchReports(db),
      countBenchReports(db),
      listScenes(db),
      benchTargets(db),
    ]);
    const titles = new Map(scenes.map((s) => [s.id, s.title]));
    reports = await Promise.all(
      rows.map(async (row) => toReportVM(row, titles, await reportShotHrefs(row))),
    );
    reportsTotal = counts.runs;
    sandboxTotal = counts.sandboxRuns;
    targets = targetRows;
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({ badge: sidBadge(sid), unavailable, reports, reportsTotal, sandboxTotal, targets });
}

export default function FoundryBench({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("fd_console_module_viewed", { module: "bench" }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  return (
    <FdBenchPage
      reports={d.reports}
      reportsTotal={d.reportsTotal}
      sandboxTotal={d.sandboxTotal}
      targets={d.targets}
      unavailable={d.unavailable}
      onReportOpen={(report) =>
        track(
          "fd_bench_report_viewed",
          {
            slug: report.slug,
            // An arena run's verdict is a process exit code, not a test of the
            // game, and the card suppresses its pill. Report "none" rather than a
            // bare "pass" the reader never saw.
            verdict: report.runner === "arena" ? "none" : report.verdict ?? "none",
          },
          ctx,
        )
      }
    />
  );
}
