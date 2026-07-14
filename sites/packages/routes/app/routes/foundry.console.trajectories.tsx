import { useEffect, useRef } from "react";

import { evidenceLabel } from "@ui/foundry/components/evidence";
import type { FdTrajectoryRowVM } from "@ui/foundry/components/FdTrajectoryRow";
import FdTrajectoriesPage, {
  type FdTrajectoriesFilterVM,
} from "@ui/foundry/pages/FdTrajectoriesPage";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdstat.css";
import "@ui/foundry/components/fdtable.css";
import "@ui/foundry/components/fdtrajectoryrow.css";
import "@ui/foundry/pages/fdtrajectories.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import {
  FoundryUnavailableError,
  getPool,
  sidBadge,
} from "@data/lib/foundry/db.server";
import { getScene } from "@data/lib/foundry/scenes.server";
import {
  countTrajectories,
  listTrajectories,
  type TrajectoryListRow,
} from "@data/lib/foundry/trajectory.server";

import type { Route } from "./+types/foundry.console.trajectories";

const LIST_LIMIT = 200;

export function meta() {
  return [{ title: "Run logs — The Foundry" }];
}

function toRowVM(row: TrajectoryListRow): FdTrajectoryRowVM {
  return {
    id: row.id,
    sceneTitle: row.sceneTitle,
    sceneId: row.sceneId,
    gameHref: row.sceneId ? `/foundry/play/${encodeURIComponent(row.sceneId)}` : null,
    provenance: row.provenance,
    runner: row.runner,
    events: row.events,
    finishReason: row.finishReason,
    parentTrajectoryId: row.parentTrajectoryId,
    seedLength: row.seedLength,
    // Label only — the raw absolute evidence path never leaves the server.
    evidence: row.evidencePath ? evidenceLabel(row.evidencePath) : null,
    evidenceHref: row.evidencePath
      ? `/foundry/console/evidence/${encodeURIComponent(row.id)}`
      : null,
    createdAt: row.createdAt,
  };
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const sceneParam = new URL(request.url).searchParams.get("scene")?.trim() || null;

  let records: FdTrajectoryRowVM[] = [];
  let total = 0;
  let filter: FdTrajectoriesFilterVM | null = null;
  let unavailable = false;

  try {
    const db = getPool();
    const [rows, count, scene] = await Promise.all([
      listTrajectories(db, {
        limit: LIST_LIMIT,
        ...(sceneParam ? { sceneId: sceneParam } : {}),
      }),
      countTrajectories(db, sceneParam ? { sceneId: sceneParam } : {}),
      sceneParam ? getScene(db, sceneParam) : null,
    ]);
    records = rows.map(toRowVM);
    total = count;
    // An unknown slug keeps the filter chip (labeled by the raw param) over the
    // normal empty table — never a 404, never an invented title.
    if (sceneParam) {
      filter = {
        sceneId: sceneParam,
        title: scene?.title ?? null,
        gameHref: scene ? `/foundry/play/${scene.id}` : null,
        clearHref: "/foundry/console/trajectories",
      };
    }
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({ badge: sidBadge(sid), unavailable, records, total, filter });
}

export default function FoundryTrajectories({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("fd_console_module_viewed", { module: "trajectories" }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  return (
    <FdTrajectoriesPage
      records={d.records}
      total={d.total}
      filter={d.filter}
      unavailable={d.unavailable}
      onOpen={(trajectoryId, provenance) =>
        track("fd_trajectory_inspected", { provenance, trajectory_id: trajectoryId }, ctx)
      }
    />
  );
}
