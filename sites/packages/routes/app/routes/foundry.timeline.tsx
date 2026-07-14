import { humanizeEntityTitle } from "@ui/foundry/components/FdGameCard";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";
import FdTimelinePage, {
  type FdTimelineRowVM,
  type FdTimelineStatsVM,
} from "@ui/foundry/pages/FdTimelinePage";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdstat.css";
import "@ui/foundry/pages/fdtimeline.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { FoundryUnavailableError, sidBadge } from "@data/lib/foundry/db.server";
import {
  TIMELINE_LANES,
  countTimelineSince,
  listTimeline,
  markTimelineVisit,
  timelineStats,
} from "@data/lib/foundry/memory.server";
import type { TimelineLane, TimelineRow } from "@data/lib/foundry/types";

import type { Route } from "./+types/foundry.timeline";

export function meta() {
  return [{ title: "Community memory — The Foundry" }];
}

function parseLane(value: string | null): TimelineLane | null {
  return value && (TIMELINE_LANES as readonly string[]).includes(value)
    ? (value as TimelineLane)
    : null;
}

// Community and worlds rows are memory-server records (action_log /
// scene_changelog) and resolve at their own /foundry/timeline/<eventId> page
// (see getMemoryEvent). scene_note is excluded: its changelog arm already
// carries the note verbatim, so the action_log row has no separate permalink.
// Every other lane already links its subject to that subject's own detail
// page (the ask, the trajectory, the doc) via subjectHref below.
const SCENE_MEMORY_ACTIONS = new Set([
  "claim_steward",
  "release_steward",
  "offer_transfer",
  "revoke_transfer",
  "accept_transfer",
]);

export function eventHref(r: TimelineRow): string | null {
  if (r.lane === "worlds") {
    const m = /^cl-(\d+)$/.exec(r.id);
    return m ? `/foundry/timeline/c${m[1]}` : null;
  }
  if (r.lane === "community" && r.subjectKind === "scene" && SCENE_MEMORY_ACTIONS.has(r.action)) {
    const m = /^al-(\d+)$/.exec(r.id);
    return m ? `/foundry/timeline/a${m[1]}` : null;
  }
  return null;
}

// bodyFor() in memory.server names the actions it knows; the doc actions land
// in its fallback, which prints the stored enum. No raw machine string reaches
// a reader, so they are named here and an action neither side names is stated
// as the absence it is.
const DOC_ACTION_BODY: Record<string, string> = {
  publish_gdd_draft: "published a design-doc draft",
  edit_gdd_doc: "edited a design doc",
};

export function rowBody(r: TimelineRow): string {
  const named = DOC_ACTION_BODY[r.action];
  if (named) return named;
  return r.body.endsWith("detail not rendered")
    ? "recorded an action this feed has no name for"
    : r.body;
}

// The feed's only possible next action: each subject label links to the surface
// that shows the row the event was read from.
function subjectHref(r: TimelineRow): string | null {
  if (r.lane === "community") {
    if (r.subjectKind === "scene" && r.subject) return `/foundry/play/${r.subject}`;
    if (r.subjectKind === "session") return "/foundry/sessions";
    if (r.subjectKind === "request") {
      return r.subject ? `/foundry/exchange/${r.subject}` : "/foundry/exchange";
    }
    if (r.subjectKind === "doc" && r.subject) return `/foundry/gdd/${r.subject}`;
    return null;
  }
  if (r.lane === "exchange") {
    return r.subject ? `/foundry/exchange/${r.subject}` : "/foundry/exchange";
  }
  if (r.lane === "worlds" && r.subject) return `/foundry/play/${r.subject}`;
  if (r.lane === "docs") return `/foundry/gdd/${r.id.replace(/^gd-/, "")}`;
  if (r.lane === "trajectory") {
    return `/foundry/console/trajectories/${r.id.replace(/^tr-/, "")}`;
  }
  if (r.lane === "harness") {
    return `/foundry/console/evidence/${r.id.replace(/^br-/, "")}`;
  }
  return null;
}

export async function loader({ request }: Route.LoaderArgs) {
  const base = sidLoader(request);
  const url = new URL(request.url);
  const lane = parseLane(url.searchParams.get("lane"));
  // An unparsable cursor falls back to the top of the lane, mirroring how a
  // bogus ?lane falls back to the merged feed — never a 500 off query text.
  // Date.parse also accepts extended-year forms (year 0, signed six-digit
  // years) whose ISO strings postgres refuses — clamp to a sane window so
  // those fall back too instead of escaping as a DB error. Date.UTC(1,…)
  // maps two-digit years to 1900+, so the effective window is 1901-01-01 to
  // 9999-12-31T00:00 — centuries wider than any real cursor either way.
  const raw = url.searchParams.get("before");
  const parsed = raw ? Date.parse(raw) : NaN;
  const inRange =
    !Number.isNaN(parsed) &&
    parsed >= Date.UTC(1, 0, 1) &&
    parsed <= Date.UTC(9999, 11, 31);
  const before = inRange ? new Date(parsed).toISOString() : null;

  try {
    // The visit marker moves only on the merged top-of-feed load — a lane
    // filter or an older page is the same visit continuing, not a new one.
    const isFrontDoor = lane === null && before === null;
    // The delta line is garnish on the feed: any failure in the marker or the
    // count renders the feed without it, never the error page instead of it.
    const [page, stats, prevVisit] = await Promise.all([
      listTimeline({ before, lanes: lane ? [lane] : undefined, limit: 50 }),
      timelineStats(),
      isFrontDoor
        ? markTimelineVisit(base.sid).catch(() => null)
        : Promise.resolve(null),
    ]);
    const sinceVisit =
      prevVisit === null
        ? null
        : await countTimelineSince(prevVisit)
            .then((fresh) => ({ prev: prevVisit, fresh }))
            .catch(() => null);
    const rows: FdTimelineRowVM[] = page.rows.map((r) => ({
      id: r.id,
      lane: r.lane,
      at: r.at,
      actor: r.actor,
      body: rowBody(r),
      // Scene subjects render the shelf's display form of the entity title;
      // every other subject keeps its stored label verbatim.
      subjectLabel:
        r.subjectLabel !== null &&
        (r.lane === "worlds" ||
          r.lane === "harness" ||
          r.lane === "trajectory" ||
          (r.lane === "community" && r.subjectKind === "scene"))
          ? humanizeEntityTitle(r.subjectLabel)
          : r.subjectLabel,
      subjectHref: subjectHref(r),
      eventHref: eventHref(r),
      machineMade: r.provenance !== "visitor",
      sandbox: r.runner === "arena",
      dateOnly: r.dateOnly,
    }));
    const vm: FdTimelineStatsVM = {
      events: stats.events,
      actors: stats.actors,
      firstMemory: stats.firstMemory,
    };
    track("fd_timeline_viewed", { lane }, { sid: sidBadge(base.sid) });
    return base.wrap({
      unavailable: false,
      rows,
      stats: vm,
      lane,
      before,
      nextBefore: page.nextBefore,
      sinceVisit,
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    return base.wrap({
      unavailable: true,
      rows: [] as FdTimelineRowVM[],
      stats: { events: 0, actors: 0, firstMemory: null } as FdTimelineStatsVM,
      lane,
      before,
      nextBefore: null,
      sinceVisit: null,
    });
  }
}

export default function FoundryTimeline({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  if (d.unavailable) {
    return (
      <div className="fd-page fd-stack fd-timeline">
        <FdPageHead title="Community memory" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }
  return (
    <FdTimelinePage
      rows={d.rows}
      stats={d.stats}
      lane={d.lane}
      before={d.before}
      nextBefore={d.nextBefore}
      sinceVisit={d.sinceVisit}
    />
  );
}
