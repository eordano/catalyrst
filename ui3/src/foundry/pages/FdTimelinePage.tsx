import { dayUTC, groupDigits, plural, stampUTC } from "../fmt";
import Button from "../../atoms/Button";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import FdTime from "../components/FdTime";
import "./fdtimeline.css";

export type FdTimelineLane =
  | "community"
  | "exchange"
  | "worlds"
  | "harness"
  | "trajectory"
  | "docs";

export type FdTimelineActor =
  | { name: string }
  | { badge: string }
  | { source: string };

export type FdTimelineRowVM = {
  id: string;
  lane: FdTimelineLane;
  at: string;
  actor: FdTimelineActor;
  body: string;
  subjectLabel: string | null;
  /** The surface that shows the row this event was read from. */
  subjectHref: string | null;
  /** This event's own permalink page, when the row is a memory-server record
   *  (action_log / scene_changelog) that resolves one — null for lanes with
   *  no /foundry/timeline/<eventId> page of their own. */
  eventHref: string | null;
  machineMade: boolean;
  /** A sandbox simulation — labeled in the feed; bench-run counts exclude it,
   *  episode counts include it (the arena-row rule in bench.server.ts). */
  sandbox: boolean;
  /** The source recorded only a DAY for this event — a stored fact set at
   *  import time, so the stamp shows the day alone, never an invented
   *  midnight. A machine lane's genuine midnight timestamp keeps its clock. */
  dateOnly: boolean;
};

export type FdTimelineStatsVM = {
  events: number;
  actors: number;
  firstMemory: string | null;
};

/** One name per lane, rendered identically in the filter and in the row chip —
 *  the canonical vocabulary: a recorded bot playthrough is a run, its stored
 *  record a run log. */
export const LANE_LABEL: Record<FdTimelineLane, string> = {
  community: "community",
  exchange: "exchange",
  worlds: "worlds",
  harness: "run",
  trajectory: "run log",
  docs: "design doc",
};

const LANES: readonly FdTimelineLane[] = [
  "community",
  "exchange",
  "worlds",
  "harness",
  "trajectory",
  "docs",
];

const LANE_EMPTY: Record<FdTimelineLane | "all", string> = {
  all: "Nothing recorded yet.",
  community: "No visitor has acted here yet.",
  exchange: "No ask has been imported from a public thread yet.",
  worlds: "No game has been deployed to a World yet.",
  harness: "No bot run has been ingested yet.",
  trajectory: "No run log has been recorded yet.",
  docs: "No design doc has been imported or drafted yet.",
};

function laneHref(lane: FdTimelineLane | null): string {
  return lane ? `?lane=${lane}` : "?";
}

function actorLabel(a: FdTimelineActor): { text: string; kind: string } {
  if ("name" in a) return { text: a.name, kind: "name" };
  if ("badge" in a) return { text: `visitor ${a.badge}`, kind: "badge" };
  return { text: a.source, kind: "source" };
}

// One stamp shape across the feed. A date-only row — an import whose source
// recorded a day but no clock time, a stored fact the importer sets — renders
// the day alone rather than an invented "00:00 UTC".
export function rowStamp(iso: string, dateOnly = false): string {
  return (dateOnly ? dayUTC(iso) : stampUTC(iso)) ?? iso;
}

export type FdTimelinePageProps = {
  rows: readonly FdTimelineRowVM[];
  stats: FdTimelineStatsVM;
  lane: FdTimelineLane | null;
  /** The active ?before cursor, if any — an empty page under a cursor means
   *  "nothing older", not "nothing recorded". */
  before?: string | null;
  nextBefore: string | null;
  /** The visitor's previous timeline visit and how many events landed since —
   *  from the stored visit marker. Null on a first visit (no invented
   *  baseline) and on filtered/paged views (same visit continuing). */
  sinceVisit?: { prev: string; fresh: number } | null;
};

export default function FdTimelinePage({
  rows,
  stats,
  lane,
  before = null,
  nextBefore,
  sinceVisit = null,
}: FdTimelinePageProps) {
  return (
    <div className="fd-page fd-stack fd-timeline">
      <FdPageHead title="Community memory" />

      {sinceVisit ? (
        <p className="fd-note">
          {sinceVisit.fresh > 0
            ? `${plural(sinceVisit.fresh, "event")} since your last visit`
            : "Nothing new since your last visit"}{" "}
          (
          <FdTime iso={sinceVisit.prev} title={stampUTC(sinceVisit.prev)}>
            {dayUTC(sinceVisit.prev)}
          </FdTime>
          )
        </p>
      ) : null}

      <div className="fd-statrow">
        <FdStat
          label="Events remembered"
          value={groupDigits(stats.events)}
          note="Across every lane, filter or not."
          mono
        />
        <FdStat
          label="Visitors who acted"
          value={groupDigits(stats.actors)}
          note="Across every lane, filter or not."
          mono
        />
        <FdStat
          label="First memory"
          value={dayUTC(stats.firstMemory) ?? "—"}
          note="The oldest event on record."
        />
      </div>

      <nav className="fd-timeline__lanes" aria-label="Filter by lane">
        <a
          className={"fd-chip fd-chip--mono" + (lane === null ? " is-on" : "")}
          href={laneHref(null)}
          aria-current={lane === null ? "true" : undefined}
        >
          all
        </a>
        {LANES.map((l) => (
          <a
            key={l}
            className={"fd-chip fd-chip--mono" + (lane === l ? " is-on" : "")}
            href={laneHref(l)}
            aria-current={lane === l ? "true" : undefined}
          >
            {LANE_LABEL[l]}
          </a>
        ))}
      </nav>

      <FdSection
        title={lane ? `The ${LANE_LABEL[lane]} lane` : "The merged memory"}
        badge={
          rows.length > 0 ? (
            <span className="fd-chip" title="events on this page">
              {plural(rows.length, "event")}
            </span>
          ) : undefined
        }
        aside={
          nextBefore ? (
            <Button
              as="a"
              variant="secondary"
              size="sm"
              href={
                (lane ? `?lane=${lane}&` : "?") +
                `before=${encodeURIComponent(nextBefore)}`
              }
            >
              Older events
            </Button>
          ) : undefined
        }
      >
        {rows.length === 0 ? (
          before ? (
            <p className="fd-empty">
              Nothing older than this point —{" "}
              <a href={laneHref(lane)}>back to the newest</a>.
            </p>
          ) : (
            <p className="fd-empty">{LANE_EMPTY[lane ?? "all"]}</p>
          )
        ) : (
          <ol className="fd-timeline__list">
            {rows.map((r) => {
              const actor = actorLabel(r.actor);
              // The stored runner value 'arena' is an internal name, so the row
              // reads "sandbox runner" beside its sandbox chip.
              if (r.sandbox && actor.kind === "source" && actor.text === "arena") {
                actor.text = "sandbox runner";
              }
              if (actor.kind === "source" && actor.text === "dclbots") {
                actor.text = "playtest harness";
              }
              return (
                <li
                  key={r.id}
                  className={"fd-timeline__row" + (r.machineMade ? " is-machine" : "")}
                >
                  {r.eventHref ? (
                    <a className="fd-timeline__at is-link" href={r.eventHref}>
                      <FdTime iso={r.at} title={r.at}>
                        {rowStamp(r.at, r.dateOnly)}
                      </FdTime>
                    </a>
                  ) : (
                    <FdTime iso={r.at} className="fd-timeline__at" title={r.at}>
                      {rowStamp(r.at, r.dateOnly)}
                    </FdTime>
                  )}
                  <span className="fd-chip fd-chip--mono fd-timeline__lanechip">
                    {LANE_LABEL[r.lane]}
                  </span>
                  <div className="fd-timeline__main">
                    <span className={"fd-timeline__actor is-" + actor.kind}>
                      {actor.text}
                    </span>{" "}
                    <span>{r.body}</span>
                    {r.subjectLabel &&
                    !r.body.includes(r.subjectLabel) &&
                    !r.body.includes(r.subjectLabel.replace(/ /g, "_")) ? (
                      <span className="fd-timeline__subject">
                        {" — "}
                        {r.subjectHref ? (
                          <a href={r.subjectHref}>{r.subjectLabel}</a>
                        ) : (
                          r.subjectLabel
                        )}
                      </span>
                    ) : null}
                    {r.sandbox ? (
                      <>
                        {" "}
                        <span className="fd-chip">sandbox</span>
                      </>
                    ) : null}
                  </div>
                </li>
              );
            })}
          </ol>
        )}
      </FdSection>
    </div>
  );
}
