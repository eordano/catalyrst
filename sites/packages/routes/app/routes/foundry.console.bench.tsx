import { useEffect, useRef } from "react";

import EmptyState from "@ui/components/EmptyState";
import { evidenceLabel } from "@ui/foundry/components/evidence";
import FdBenchPage, { type FdBenchReportVM } from "@ui/foundry/pages/FdBenchPage";

import "@ui/components/emptystate.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdverdictpill.css";
import "@ui/foundry/pages/fdbench.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { countBenchReports, listBenchReports } from "@data/lib/foundry/bench.server";
import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import type { BotReport } from "@data/lib/foundry/types";

import type { Route } from "./+types/foundry.console.bench";

export function meta() {
  return [{ title: "Bot bench — The Foundry" }];
}

function toReportVM(report: BotReport): FdBenchReportVM {
  return {
    id: report.id,
    slug: report.slug,
    runner: report.runner,
    realm: report.realm,
    ranAt: report.ranAt,
    verdict: report.verdict,
    checksTotal: report.checksTotal,
    checksFailed: report.checksFailed,
    missingTools: report.missingTools,
    stubbedTools: report.stubbedTools,
    networkWrites: report.networkWrites,
    shots: report.shots.length,
    // Label only — the raw absolute evidence path never leaves the server.
    evidence: report.evidencePath ? evidenceLabel(report.evidencePath) : null,
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
  let unavailable = false;

  try {
    const db = getPool();
    const [rows, total] = await Promise.all([
      listBenchReports(db),
      countBenchReports(db),
    ]);
    reports = rows.map(toReportVM);
    reportsTotal = total;
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({ badge: sidBadge(sid), unavailable, reports, reportsTotal });
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

  if (d.unavailable) {
    return (
      <EmptyState
        variant="inline"
        title="Foundry database not configured"
        subtitle="Bot runs are recorded in Postgres. Set FOUNDRY_DATABASE_URL on this deployment to read them."
      />
    );
  }

  return (
    <FdBenchPage
      reports={d.reports}
      reportsTotal={d.reportsTotal}
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
