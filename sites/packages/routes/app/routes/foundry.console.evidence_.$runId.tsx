import { evidenceLabel } from "@ui/foundry/components/evidence";
import FdEvidencePage, {
  type FdEvidenceShot,
} from "@ui/foundry/pages/FdEvidencePage";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdevidence.css";

import { sidLoader } from "@core/lib/experiments/story-loader";

import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import {
  inspectEvidenceDir,
  resolveEvidenceRun,
} from "@data/lib/foundry/evidence.server";
import { getScene } from "@data/lib/foundry/scenes.server";

import type { Route } from "./+types/foundry.console.evidence_.$runId";

const RUNS_HREF = "/foundry/console/bench";

export function meta() {
  return [{ title: "Evidence — The Foundry" }];
}

export async function loader({ params, request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const runId = params.runId;

  let unavailable = false;
  let missing = false;
  let title: string | null = null;
  let gameHref: string | null = null;
  let label: string | null = null;
  let present = false;
  let shots: FdEvidenceShot[] = [];
  let logTail: string[] | null = null;
  let logLines: number | null = null;
  let logHref: string | null = null;
  let dataSummary: { key: string; value: string }[] | null = null;
  let replayHref: string | null = null;

  try {
    const db = getPool();
    const run = await resolveEvidenceRun(db, runId);
    // An unknown id keeps the console around the reader — layout, rail and one
    // line — instead of ejecting to the site-wide error page.
    if (!run) {
      missing = true;
    } else {
      replayHref =
        run.kind === "trajectory" ? `/foundry/console/trajectories/${run.runId}` : null;
      if (run.sceneId) {
        const scene = await getScene(db, run.sceneId);
        title = scene?.title ?? null;
        gameHref = scene ? `/foundry/play/${encodeURIComponent(scene.id)}` : null;
      }
      if (run.evidencePath) {
        // The absolute path stays server-side: the page gets the label, and each
        // surviving file a sanitized /file/ URL under this run id.
        label = evidenceLabel(run.evidencePath);
        const listing = await inspectEvidenceDir(run.evidencePath);
        present = listing.present;
        shots = listing.shots.map((name) => ({
          name,
          url: `/foundry/console/evidence/${encodeURIComponent(runId)}/file/shots/${encodeURIComponent(name)}`,
        }));
        logTail = listing.logTail;
        logLines = listing.logLines;
        // The served file passes through the same path redaction as the tail, so
        // linking it leaks nothing the tail already withholds.
        logHref = listing.logTail
          ? `/foundry/console/evidence/${encodeURIComponent(runId)}/file/run.log`
          : null;
        dataSummary = listing.dataSummary;
      }
    }
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap(
    {
      badge: sidBadge(sid),
      unavailable,
      missing,
      runId,
      title,
      gameHref,
      label,
      present,
      shots,
      logTail,
      logLines,
      logHref,
      dataSummary,
      replayHref,
    },
    missing ? { status: 404 } : undefined,
  );
}

export default function FoundryEvidence({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  return (
    <FdEvidencePage
      runId={d.runId}
      title={d.title}
      gameHref={d.gameHref}
      label={d.label}
      present={d.present}
      shots={d.shots}
      logTail={d.logTail}
      logLines={d.logLines}
      logHref={d.logHref}
      dataSummary={d.dataSummary}
      replayHref={d.replayHref}
      backHref={RUNS_HREF}
      unavailable={d.unavailable}
      missing={d.missing}
    />
  );
}
