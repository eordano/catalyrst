import { useEffect, useRef } from "react";

import EmptyState from "@ui/components/EmptyState";
import FdTrajectoriesPage from "@ui/foundry/pages/FdTrajectoriesPage";

import "@ui/components/emptystate.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdtrajectories.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import {
  FoundryUnavailableError,
  getPool,
  sidBadge,
} from "@data/lib/foundry/db.server";
import {
  countTrajectories,
  listTrajectories,
  type TrajectoryListRow,
} from "@data/lib/foundry/trajectory.server";

import type { Route } from "./+types/foundry.console.trajectories";

const LIST_LIMIT = 200;

export function meta() {
  return [{ title: "Trajectories — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let records: TrajectoryListRow[] = [];
  let total = 0;
  let unavailable = false;

  try {
    const db = getPool();
    [records, total] = await Promise.all([
      listTrajectories(db, { limit: LIST_LIMIT }),
      countTrajectories(db),
    ]);
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({ badge: sidBadge(sid), unavailable, records, total });
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

  if (d.unavailable) {
    return (
      <EmptyState
        variant="inline"
        title="Foundry database not configured"
        subtitle="Episodes live in Postgres. Set FOUNDRY_DATABASE_URL on this deployment to read them."
      />
    );
  }

  return (
    <FdTrajectoriesPage
      records={d.records}
      total={d.total}
      onOpen={(trajectoryId, provenance) =>
        track("fd_trajectory_inspected", { provenance, trajectory_id: trajectoryId }, ctx)
      }
    />
  );
}
