import { useCallback, useEffect, useRef, useState } from "react";
import { data } from "react-router";

import EmptyState from "@ui/components/EmptyState";
import { evidenceLabel } from "@ui/foundry/components/evidence";
import FdTrajectoryReplay from "@ui/foundry/pages/FdTrajectoryReplay";

import "@ui/components/emptystate.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/pages/fdtrajectoryreplay.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import {
  FoundryUnavailableError,
  getPool,
  sidBadge,
} from "@data/lib/foundry/db.server";
import {
  countTrajectoryEvents,
  getTrajectoryHeader,
  listTrajectoryEvents,
  REPLAY_EVENT_LIMIT,
  TrajectoryLogError,
} from "@data/lib/foundry/trajectory.server";
import type { Trajectory, TrajectoryEvent } from "@data/lib/foundry/types";

import type { Route } from "./+types/foundry.console.trajectories_.$id";

const LIST_HREF = "/foundry/console/trajectories";

export function meta() {
  return [{ title: "Replay — The Foundry" }];
}

export async function loader({ params, request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const id = params.id;

  let header: (Trajectory & { sceneTitle: string | null }) | null = null;
  let events: TrajectoryEvent[] = [];
  let eventCount = 0;
  let oversize = false;
  let unavailable = false;
  let logError: string | null = null;

  try {
    const db = getPool();
    const [raw, count] = await Promise.all([
      getTrajectoryHeader(db, id),
      countTrajectoryEvents(db, id),
    ]);
    if (!raw) throw data(null, { status: 404 });

    // Redact the host evidence path server-side: only the basename+hash label
    // may reach the client, never the absolute /tmp+session scratch path.
    header = {
      ...raw,
      evidencePath: raw.evidencePath ? evidenceLabel(raw.evidencePath) : null,
    };
    eventCount = count;
    oversize = count > REPLAY_EVENT_LIMIT;
    events = oversize ? [] : await listTrajectoryEvents(db, id);
  } catch (err) {
    if (err instanceof FoundryUnavailableError) unavailable = true;
    else if (err instanceof TrajectoryLogError) logError = err.message;
    else throw err;
  }

  return wrap({
    badge: sidBadge(sid),
    unavailable,
    logError,
    header,
    events,
    eventCount,
    oversize,
    limit: REPLAY_EVENT_LIMIT,
  });
}

export default function FoundryTrajectoryReplay({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };
  const maxSeq = d.events.length === 0 ? 0 : d.events[d.events.length - 1].seq;
  const [cursor, setCursor] = useState(maxSeq);
  const lastScrub = useRef(0);

  const trajectoryId = d.header?.id ?? "";
  const opened = useRef(false);
  useEffect(() => {
    if (opened.current || !trajectoryId) return;
    opened.current = true;
    track(
      "fd_replay_opened",
      { events: d.eventCount, trajectory_id: trajectoryId },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [trajectoryId]);

  const onCursor = useCallback(
    (seq: number) => {
      setCursor(seq);
      const now = Date.now();
      if (now - lastScrub.current < 1000) return;
      lastScrub.current = now;
      track("fd_replay_scrubbed", { seq, trajectory_id: trajectoryId }, ctx);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [trajectoryId],
  );

  const onStep = useCallback(
    (direction: "back" | "forward") => {
      setCursor((current) => {
        const next = direction === "back" ? current - 1 : current + 1;
        return Math.min(Math.max(next, 0), maxSeq);
      });
      track("fd_replay_stepped", { direction, trajectory_id: trajectoryId }, ctx);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [maxSeq, trajectoryId],
  );

  if (d.unavailable) {
    return (
      <EmptyState
        variant="inline"
        title="Foundry database not configured"
        subtitle="Episodes live in Postgres. Set FOUNDRY_DATABASE_URL on this deployment to read them."
      />
    );
  }

  if (d.logError || !d.header) {
    return (
      <EmptyState
        variant="inline"
        title="This episode cannot be replayed"
        subtitle={
          d.logError ??
          "The log could not be read. Nothing is reconstructed in its place."
        }
        actions={[{ label: "All episodes", href: LIST_HREF, variant: "outline" }]}
      />
    );
  }

  return (
    <FdTrajectoryReplay
      header={d.header}
      events={d.events}
      eventCount={d.eventCount}
      oversize={d.oversize}
      limit={d.limit}
      cursor={cursor}
      onCursor={onCursor}
      onStep={onStep}
      backHref={LIST_HREF}
    />
  );
}
