import { useCallback, useEffect, useRef, useState } from "react";

import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";
import { evidenceLabel } from "@ui/foundry/components/evidence";
import FdTrajectoryReplay, {
  type FdReplayFrame,
} from "@ui/foundry/pages/FdTrajectoryReplay";

import "@ui/foundry/components/fdeventrow.css";
import "@ui/foundry/components/fdreplayscrubber.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdstat.css";
import "@ui/foundry/pages/fdtrajectoryreplay.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import {
  FoundryUnavailableError,
  getPool,
  sidBadge,
} from "@data/lib/foundry/db.server";
import { inspectEvidenceDir } from "@data/lib/foundry/evidence.server";
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
  return [{ title: "Run log — The Foundry" }];
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
  let frames: FdReplayFrame[] = [];
  let evidenceGone = false;
  let notFound = false;

  try {
    const db = getPool();
    const [raw, count] = await Promise.all([
      getTrajectoryHeader(db, id),
      countTrajectoryEvents(db, id),
    ]);
    // An unknown id keeps the console around the reader — layout, rail and one
    // line — instead of ejecting to the site-wide error page.
    if (!raw) {
      notFound = true;
    } else {
      // Redact the host evidence path server-side: only the basename+hash label
      // may reach the client, never the absolute /tmp+session scratch path.
      header = {
        ...raw,
        evidencePath: raw.evidencePath ? evidenceLabel(raw.evidencePath) : null,
      };
      eventCount = count;
      oversize = count > REPLAY_EVENT_LIMIT;
      events = oversize ? [] : await listTrajectoryEvents(db, id);

      // The filmstrip shows only frames that actually survive on this host; a
      // recorded-but-vanished directory becomes an honest absence line instead.
      if (raw.evidencePath) {
        const listing = await inspectEvidenceDir(raw.evidencePath);
        if (listing.present) {
          frames = listing.shots.map((name) => ({
            name,
            url: `/foundry/console/evidence/${encodeURIComponent(id)}/file/shots/${encodeURIComponent(name)}`,
          }));
        } else {
          evidenceGone = true;
        }
      }
    }
  } catch (err) {
    if (err instanceof FoundryUnavailableError) unavailable = true;
    else if (err instanceof TrajectoryLogError) logError = err.message;
    else throw err;
  }

  return wrap(
    {
      badge: sidBadge(sid),
      unavailable,
      logError,
      header,
      events,
      eventCount,
      oversize,
      limit: REPLAY_EVENT_LIMIT,
      frames,
      evidenceGone,
    },
    notFound ? { status: 404 } : undefined,
  );
}

export default function FoundryTrajectoryReplay({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };
  const maxSeq = d.events.length === 0 ? 0 : d.events[d.events.length - 1].seq;
  const [cursor, setCursor] = useState(maxSeq);
  const [playing, setPlaying] = useState(false);
  const lastScrub = useRef(0);

  // Keyed by episode: replay-to-replay navigation re-renders this same route
  // element, so the cursor and the opened marker must reset per episode rather
  // than carry the previous replay's state into the next one.
  const trajectoryId = d.header?.id ?? "";
  const opened = useRef<string | null>(null);
  useEffect(() => {
    if (!trajectoryId || opened.current === trajectoryId) return;
    opened.current = trajectoryId;
    setCursor(maxSeq);
    setPlaying(false);
    track(
      "fd_replay_opened",
      { events: d.eventCount, trajectory_id: trajectoryId },
      ctx,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [trajectoryId]);

  // Playback: auto-advance the cursor through the recorded events, about two a
  // second, and stop at the end of the log. Nothing beyond the log is played.
  useEffect(() => {
    if (!playing) return;
    const timer = window.setInterval(() => {
      setCursor((current) => {
        const next = d.events.find((e) => e.seq > current);
        if (!next) {
          setPlaying(false);
          return current;
        }
        return next.seq;
      });
    }, 500);
    return () => window.clearInterval(timer);
  }, [playing, d.events]);

  const onPlayToggle = useCallback(() => {
    setPlaying((was) => {
      // Play from a cursor already at the end restarts from the top.
      if (!was && cursor >= maxSeq) setCursor(0);
      return !was;
    });
  }, [cursor, maxSeq]);

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

  if (d.unavailable || d.logError || !d.header) {
    return (
      <div className="fd-stack">
        <FdPageHead
          eyebrow="Run logs"
          title="Run log"
          crumbs={<a href={LIST_HREF}>← All runs</a>}
        />
        <p className="fd-empty">
          {d.unavailable ? FD_UNAVAILABLE : "This run’s log could not be read."}
        </p>
      </div>
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
      gameHref={
        d.header.sceneId
          ? `/foundry/play/${encodeURIComponent(d.header.sceneId)}`
          : null
      }
      evidenceHref={
        d.header.evidencePath
          ? `/foundry/console/evidence/${d.header.id}`
          : null
      }
      frames={d.frames}
      evidenceGone={d.evidenceGone}
      playing={playing}
      onPlayToggle={onPlayToggle}
    />
  );
}
