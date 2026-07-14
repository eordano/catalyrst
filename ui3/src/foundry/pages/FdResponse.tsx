import { dayUTC, plural, stampUTC } from "../fmt";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import {
  JOB_NAMES,
  SIGNATURE_JOBS,
  jobTitle,
  type FdEmotionalJobLetter,
  type FdEmotionalJobVM,
} from "../components/FdEmotionalJobs";
import {
  MARKET_CELL_NAMES,
  humanizeEntityTitle,
  marketCellTitle,
  type FdMarketCellSlug,
  type FdMarketCellVM,
} from "../components/FdGameCard";
import "./fdresponse.css";

// One game's readings page: who came, what mattered, what changed — each line a
// measured fact or a stated absence, linked to where it comes from. Nothing on
// this page is a proxy: a signal nobody measured renders as its absence and the
// reason, never as a zero pretending relevance. There is no input here by
// design — a player responds through the exchange, and the head says so.

export type FdResponseVisitsVM = {
  days: readonly { day: string; visitors: number; returning: number }[];
  totalEvents: number;
  distinctVisitors: number;
};

export type FdResponseReplayVM = {
  trajectoryId: string;
  opens: number;
  interactions: number;
  /** The replayed run's own date, resolved by the loader; null = the run's
   *  record was not found (the line then names only the id). */
  ranAt: string | null;
  /** The arena-row rule follows the replay: a sandbox simulation's replay is
   *  real attention on a synthetic run, and says so. */
  sandbox: boolean;
};

/** The measured signals, one connection: null = the counts are not connected
 *  at all, and every count-bearing line states that instead of a number. */
export type FdResponseSignalsVM = {
  visits: FdResponseVisitsVM;
  replays: readonly FdResponseReplayVM[];
  downloads: number;
};

export type FdResponseRunVM = {
  id: string;
  ranAt: string;
  /** Plain words with the arena rule already applied server-side. */
  text: string;
  evidenceHref: string | null;
  replayHref: string | null;
};

export type FdResponseGatheringVM = {
  seriesId: string;
  title: string;
  occurrenceAt: string;
  rsvpCount: number;
};

export type FdResponseMemoryVM = {
  eventId: string;
  at: string;
  body: string;
  sourceNote: string;
};

export type FdResponseRevisionVM =
  | { kind: "none" }
  | { kind: "thin"; deployedDay: string }
  | { kind: "split"; deployedDay: string; before: number; after: number };

/** An ask whose reading names this game as the shelf's answer — the demand
 *  link read back from the game's side. */
export type FdResponseAskAnswerVM = {
  requestId: string;
  title: string;
  readAt: string;
};

export type FdResponseProps = {
  title: string;
  slug: string;
  gameHref: string;
  /** "15 Aug 2026" — when measurement began; nothing before it exists. */
  measuredSince: string;
  signals: FdResponseSignalsVM | null;
  /** True when a configured signals connection failed to answer — renders
   *  could-not-be-read, never the unconfigured "not connected" copy. */
  signalsUnreadable?: boolean;
  gatherings: readonly FdResponseGatheringVM[];
  runs: readonly FdResponseRunVM[];
  marketCell: FdMarketCellVM | null;
  emotionalJobs: readonly FdEmotionalJobVM[] | null;
  /** Cells no game in the registry reads as; null = cells never read at all. */
  cellGaps: readonly FdMarketCellSlug[] | null;
  /** Jobs no game in the registry serves; null = jobs never read at all. */
  jobGaps: readonly FdEmotionalJobLetter[] | null;
  /** Asks whose reading names this game as the shelf's answer; [] = none. */
  askAnswers: readonly FdResponseAskAnswerVM[];
  gddHref: string | null;
  memory: readonly FdResponseMemoryVM[];
  hasVisitorNote: boolean;
  revision: FdResponseRevisionVM;
};

// Below this many measured events the per-day lines are shown but no pattern is
// claimed from them.
const TOO_FEW_EVENTS = 20;

const NOT_CONNECTED_VISITS = "Visit counts are not connected yet.";
const NOT_CONNECTED_COUNTS = "Replay and download counts are not connected yet.";
// The connection exists but did not answer — a different fact than "not
// connected", stated as such (the presence-rollup pattern).
const UNREADABLE_VISITS = "Visit counts could not be read.";
const UNREADABLE_COUNTS = "Replay and download counts could not be read.";

function WhoCameBack({
  signals,
  signalsUnreadable = false,
  gatherings,
  measuredSince,
}: Pick<FdResponseProps, "signals" | "signalsUnreadable" | "gatherings" | "measuredSince">) {
  return (
    <FdSection title="Who came back">
      <div className="fd-board">
        <div className="fd-card">
          <h4 className="fd-subhead">Visits</h4>
          {signals === null ? (
            <p className="fd-empty">{signalsUnreadable ? UNREADABLE_VISITS : NOT_CONNECTED_VISITS}</p>
          ) : (
            <>
              <div className="fd-chiprow">
                <span className="fd-chip">Measured since {measuredSince}</span>
              </div>
              {signals.visits.days.length === 0 ? (
                <p className="fd-empty">No visit has been measured yet.</p>
              ) : (
                <>
                  <ul className="fd-response__list">
                    {signals.visits.days.map((d) => (
                      <li key={d.day} className="fd-response__line">
                        <span className="fd-num">{dayUTC(d.day)}</span> —{" "}
                        {plural(d.visitors, "session")}
                        {d.returning > 0 ? `, ${d.returning} returning` : ""}
                      </li>
                    ))}
                  </ul>
                  <p className="fd-response__line">
                    {plural(signals.visits.distinctVisitors, "distinct browser session")}{" "}
                    so far.
                  </p>
                  <p className="fd-note">
                    A session is a browser cookie — this program cannot yet
                    tell a person from its own automation, so sessions are
                    never claimed as humans (
                    <a href="/foundry/deck#slide-15">deck slide 15</a>).
                  </p>
                </>
              )}
              {signals.visits.totalEvents < TOO_FEW_EVENTS ? (
                <p className="fd-empty">Too few visits yet to read a pattern.</p>
              ) : null}
            </>
          )}
        </div>

        <div className="fd-card">
          <h4 className="fd-subhead">Gatherings</h4>
          {gatherings.length === 0 ? (
            <p className="fd-empty">
              No gathering has been scheduled for this game.{" "}
              <a href="/foundry/sessions">The calendar</a>
            </p>
          ) : (
            <ul className="fd-response__list">
              {gatherings.map((g) => (
                <li
                  key={`${g.seriesId}#${g.occurrenceAt}`}
                  className="fd-response__line"
                >
                  <a href="/foundry/sessions">{g.title}</a> —{" "}
                  <FdTime iso={g.occurrenceAt}>{stampUTC(g.occurrenceAt)}</FdTime> —{" "}
                  {plural(g.rsvpCount, "guest")}
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </FdSection>
  );
}

type JobRead = FdEmotionalJobVM & { job: FdEmotionalJobLetter };

/** The three jobs the deck assigns this cell, as chips: served ones link the
 *  reading, absent ones say so. The set comes from SIGNATURE_JOBS, never
 *  re-derived. */
function QualifyStrip({
  cell,
  jobReads,
  gameHref,
}: {
  cell: NonNullable<FdMarketCellVM["cell"]>;
  jobReads: readonly JobRead[];
  gameHref: string;
}) {
  const set = SIGNATURE_JOBS[cell];
  const served = new Map(jobReads.map((r) => [r.job, r]));
  const outside = jobReads.filter((r) => !set.includes(r.job));

  return (
    <>
      <div className="fd-chiprow">
        {set.map((letter) => {
          const read = served.get(letter);
          return read ? (
            <a
              key={letter}
              className="fd-cellchip fd-cellchip--link"
              href={gameHref}
              title={jobTitle(read)}
            >
              {JOB_NAMES[letter]} · {dayUTC(read.readAt)}
              <span className="u-sr-only"> — read as served: {jobTitle(read)}</span>
            </a>
          ) : (
            <span
              key={letter}
              className="fd-cellchip"
              title="no observed design serves this yet"
            >
              {JOB_NAMES[letter]} · not served
              <span className="u-sr-only"> — no observed design serves this yet</span>
            </span>
          );
        })}
        <a
          className="fd-chip"
          href="/foundry/deck#slide-10"
          title="the deck's slide 10 assigns three jobs to each cell"
        >
          deck slide 10
        </a>
      </div>
      {outside.length > 0 ? (
        <>
          <p className="fd-label">Also read, outside this cell</p>
          <div className="fd-chiprow">
            {outside.map((r) => (
              <span key={r.job} className="fd-cellchip" title={jobTitle(r)}>
                {JOB_NAMES[r.job]} · {dayUTC(r.readAt)}
                <span className="u-sr-only"> — {jobTitle(r)}</span>
              </span>
            ))}
          </div>
        </>
      ) : null}
    </>
  );
}

function WhatMattered({
  gameHref,
  measuredSince,
  signals,
  signalsUnreadable = false,
  runs,
  marketCell,
  emotionalJobs,
  cellGaps,
  jobGaps,
  askAnswers,
  gddHref,
}: Pick<
  FdResponseProps,
  | "gameHref"
  | "measuredSince"
  | "signals"
  | "signalsUnreadable"
  | "runs"
  | "marketCell"
  | "emotionalJobs"
  | "cellGaps"
  | "jobGaps"
  | "askAnswers"
  | "gddHref"
>) {
  const jobReads = (emotionalJobs ?? []).filter(
    (r): r is JobRead => r.job !== null,
  );
  const noneRead = (emotionalJobs ?? []).find((r) => r.job === null);

  return (
    <FdSection title="What mattered">
      <div className="fd-board">
        <div className="fd-card">
          <h4 className="fd-subhead">Runs</h4>
          {runs.length === 0 ? (
            <p className="fd-empty">No recorded bot run for this game.</p>
          ) : (
            <ul className="fd-response__list">
              {runs.map((r) => (
                <li key={r.id} className="fd-response__line">
                  <FdTime iso={r.ranAt} className="fd-num">
                    {stampUTC(r.ranAt)}
                  </FdTime>{" "}
                  — {r.text}
                  {r.evidenceHref ? (
                    <>
                      {" — "}
                      <a
                        href={r.evidenceHref}
                        aria-label={`bench report, ${stampUTC(r.ranAt)} run`}
                      >
                        the report
                      </a>
                    </>
                  ) : null}
                  {r.replayHref ? (
                    <>
                      {" · "}
                      <a
                        href={r.replayHref}
                        aria-label={`replay, ${stampUTC(r.ranAt)} run`}
                      >
                        its replay
                      </a>
                    </>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="fd-card">
          <h4 className="fd-subhead">Replays and downloads</h4>
          {signals === null ? (
            <p className="fd-empty">{signalsUnreadable ? UNREADABLE_COUNTS : NOT_CONNECTED_COUNTS}</p>
          ) : (
            <>
              {signals.replays.length === 0 ? (
                <p className="fd-empty">
                  No one has opened a replay of this game&rsquo;s runs since{" "}
                  {measuredSince}.
                </p>
              ) : (
                <ul className="fd-response__list">
                  {signals.replays.map((r) => (
                    <li key={r.trajectoryId} className="fd-response__line">
                      A replay of {r.ranAt ? `the ${dayUTC(r.ranAt)} ` : "this game's "}
                      {r.sandbox ? "sandbox simulation" : "run"} was opened{" "}
                      {plural(r.opens, "time")}
                      {r.interactions > 0
                        ? `, with ${plural(r.interactions, "step")} through it`
                        : ""}{" "}
                      —{" "}
                      <a href={`/foundry/console/trajectories/${r.trajectoryId}`}>
                        watch it
                      </a>
                    </li>
                  ))}
                </ul>
              )}
              <p className="fd-response__line">
                {signals.downloads === 0 ? (
                  <>
                    No one has downloaded the scene-memory bundle since{" "}
                    {measuredSince}.
                  </>
                ) : (
                  <>
                    The scene-memory bundle — JSON, no scene bytes — was
                    downloaded {plural(signals.downloads, "time")}
                  </>
                )}
              </p>
            </>
          )}
        </div>

        <div className="fd-card">
          <h4 className="fd-subhead">This program&rsquo;s reading</h4>
          {marketCell ? (
            <>
              <div className="fd-chiprow">
                <span className="fd-cellchip" title={marketCellTitle(marketCell)}>
                  {marketCell.cell
                    ? MARKET_CELL_NAMES[marketCell.cell]
                    : "Unclassified"}{" "}
                  · {dayUTC(marketCell.classifiedAt)}
                  <span className="u-sr-only"> — {marketCellTitle(marketCell)}</span>
                </span>
              </div>
              <p className="fd-response__line">{marketCell.rationale}</p>
            </>
          ) : (
            <p className="fd-empty">Not yet read against the deck&rsquo;s market cells.</p>
          )}
          {emotionalJobs === null || emotionalJobs.length === 0 ? (
            <p className="fd-empty">
              Not yet read against the deck&rsquo;s six emotional jobs.
            </p>
          ) : jobReads.length === 0 && noneRead ? (
            <p className="fd-response__line" title={jobTitle(noneRead)}>
              Read {dayUTC(noneRead.readAt)}: none of the six jobs&rsquo; observable
              machinery is present here. {noneRead.rationale}
              <span className="u-sr-only"> — {jobTitle(noneRead)}</span>
            </p>
          ) : marketCell?.cell ? (
            <QualifyStrip
              cell={marketCell.cell}
              jobReads={jobReads}
              gameHref={gameHref}
            />
          ) : (
            <>
              <div className="fd-chiprow">
                {jobReads.map((r) => (
                  <span key={r.job} className="fd-cellchip" title={jobTitle(r)}>
                    {JOB_NAMES[r.job]} · {dayUTC(r.readAt)}
                    <span className="u-sr-only"> — {jobTitle(r)}</span>
                  </span>
                ))}
              </div>
              {marketCell ? (
                <p className="fd-note">
                  Unclassified games carry none of the three engineered jobs the deck
                  assigns per cell (
                  <a href="/foundry/deck#slide-10">deck slide 10</a>).
                </p>
              ) : null}
            </>
          )}
        </div>

        {cellGaps !== null || jobGaps !== null ? (
          <div className="fd-card">
            <h4 className="fd-subhead">Open ground across all the games</h4>
            {cellGaps !== null && cellGaps.length > 0 ? (
              <>
                <p className="fd-label">Cells</p>
                <ul className="fd-response__list">
                  {cellGaps.map((c) => (
                    <li key={c} className="fd-response__line">
                      {MARKET_CELL_NAMES[c]} — no game on{" "}
                      <a href="/foundry/play">the shelf</a> reads as this cell.
                    </li>
                  ))}
                </ul>
              </>
            ) : null}
            {jobGaps !== null && jobGaps.length > 0 ? (
              <>
                <p className="fd-label">Jobs</p>
                <ul className="fd-response__list">
                  {jobGaps.map((j) => (
                    <li key={j} className="fd-response__line">
                      {JOB_NAMES[j]} — no game on{" "}
                      <a href="/foundry/play">the shelf</a> serves this job.
                    </li>
                  ))}
                </ul>
              </>
            ) : null}
            {(cellGaps === null || cellGaps.length === 0) &&
            (jobGaps === null || jobGaps.length === 0) ? (
              <p className="fd-empty">
                Every cell and job the deck names has a game serving it —{" "}
                <a href="/foundry/play">the shelf</a>.
              </p>
            ) : null}
          </div>
        ) : null}

        <div className="fd-card">
          <h4 className="fd-subhead">On the exchange</h4>
          {askAnswers.length > 0 ? (
            <ul className="fd-response__list">
              {askAnswers.map((a) => (
                <li
                  key={a.requestId}
                  className="fd-response__line"
                  title={`This program's reading, ${a.readAt} — not the asker's`}
                >
                  Read as the shelf&rsquo;s answer to &ldquo;
                  <a href={`/foundry/exchange/${a.requestId}`}>{a.title}</a>&rdquo; —{" "}
                  {dayUTC(a.readAt)}.
                  <span className="u-sr-only">
                    {" "}
                    — this program&rsquo;s reading, not the asker&rsquo;s
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="fd-empty">
              No ask on the exchange names this game.{" "}
              <a href="/foundry/exchange">The exchange</a>
            </p>
          )}
        </div>

        <div className="fd-card">
          <h4 className="fd-subhead">Design doc</h4>
          {gddHref ? (
            <p className="fd-response__line">
              <a href={gddHref}>This game&rsquo;s design doc</a>
            </p>
          ) : (
            <p className="fd-empty">No design doc is linked to this game.</p>
          )}
        </div>
      </div>
    </FdSection>
  );
}

function WhatChanged({
  memory,
  hasVisitorNote,
  revision,
}: Pick<FdResponseProps, "memory" | "hasVisitorNote" | "revision">) {
  return (
    <FdSection title="What changed after revision">
      <div className="fd-board">
        <div className="fd-card">
          <h4 className="fd-subhead">Revisions</h4>
          {revision.kind === "split" ? (
            <p className="fd-response__line">
              Since the deploy on{" "}
              <span className="fd-num">{dayUTC(revision.deployedDay)}</span>:{" "}
              {plural(revision.after, "session")}, against{" "}
              {plural(revision.before, "session")} before it.
            </p>
          ) : revision.kind === "thin" ? (
            <p className="fd-response__line">
              A deploy landed on{" "}
              <span className="fd-num">{dayUTC(revision.deployedDay)}</span>, but too
              few visits sit on either side of it to read a change.
            </p>
          ) : (
            <p className="fd-empty">
              No revision has shipped since measurement began.
            </p>
          )}
        </div>

        <div className="fd-card">
          <h4 className="fd-subhead">This game&rsquo;s memory</h4>
          {memory.length === 0 ? (
            <p className="fd-empty">
              Nothing is recorded in this game&rsquo;s memory yet.
            </p>
          ) : (
            <ul className="fd-response__list">
              {memory.map((m) => (
                <li key={m.eventId} className="fd-response__line" title={m.sourceNote}>
                  <FdTime iso={m.at} className="fd-num">
                    {stampUTC(m.at)}
                  </FdTime>{" "}
                  — <a href={`/foundry/timeline/${m.eventId}`}>{m.body || "(no note)"}</a>
                  {m.sourceNote ? (
                    <span className="u-sr-only"> — {m.sourceNote}</span>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
          {!hasVisitorNote ? (
            <p className="fd-empty">No visitor has left a note.</p>
          ) : null}
        </div>
      </div>
    </FdSection>
  );
}

type ToolRow = {
  name: string;
  live: boolean;
  what: string;
  href?: string;
  hrefLabel?: string;
};

const TOOLS: ToolRow[] = [
  {
    name: "Demand signal",
    live: true,
    what: "Players state unmet needs and pledge to show up.",
    href: "/foundry/exchange",
    hrefLabel: "The exchange",
  },
  {
    name: "Response",
    live: true,
    what: "Who came back, what mattered, what changed — this page.",
  },
  {
    name: "First players",
    live: false,
    what: "Matched human testers for new prototypes.",
  },
  {
    name: "Clip-to-play",
    live: false,
    what: "A shared moment becomes a link into the exact session.",
  },
  {
    name: "Community import",
    live: false,
    what: "An existing group's roles and invitations, carried into a game.",
  },
  {
    name: "Session fill",
    live: false,
    what: "Nudging players toward games that are almost lively enough.",
  },
];

function ToolStrip({
  signalsConnected,
  signalsUnreadable = false,
}: {
  signalsConnected: boolean;
  signalsUnreadable?: boolean;
}) {
  return (
    <FdSection
      title="Market-making tools"
      sub={
        <>
          The six tools the deck names (
          <a href="/foundry/deck#slide-13">deck slide 13</a>), and which exist here.
        </>
      }
    >
      <ul className="fd-response__tools">
        {TOOLS.map((t) => {
          // The Response tool's own readings are on this page: with the
          // telemetry pool unconfigured, two of its three counts cannot be
          // read — the chip downgrades to what is actually delivered.
          const partly = t.name === "Response" && !signalsConnected;
          return (
            <li key={t.name} className="fd-response__tool">
              <span className="fd-cellchip fd-response__toolstate">
                {partly ? "partly live" : t.live ? "live" : "not built"}
              </span>
              <span>
                <strong>{t.name}</strong> — {t.what}
                {partly
                  ? ` Runs, gatherings and readings are measured; visit, replay and download counts ${signalsUnreadable ? "could not be read" : "are not connected yet"}.`
                  : null}
                {t.href ? (
                  <>
                    {" "}
                    <a href={t.href}>{t.hrefLabel}</a>
                  </>
                ) : null}
              </span>
            </li>
          );
        })}
      </ul>
    </FdSection>
  );
}

export default function FdResponse(props: FdResponseProps) {
  return (
    <div className="fd-page fd-stack">
      <FdPageHead
        eyebrow="Readings"
        title={humanizeEntityTitle(props.title)}
        crumbs={
          <>
            <a href="/foundry/play">← All games</a>
            {" · "}
            <a href={props.gameHref}>The game page</a>
          </>
        }
      />
      <p className="fd-response__frame">
        What this program has measured about the game — who came back, what
        mattered, what changed. To respond as a player,{" "}
        <a href="/foundry/exchange">post an ask or pledge on the Exchange</a>.
      </p>
      <WhoCameBack
        signals={props.signals}
        signalsUnreadable={props.signalsUnreadable}
        gatherings={props.gatherings}
        measuredSince={props.measuredSince}
      />
      <WhatMattered
        gameHref={props.gameHref}
        measuredSince={props.measuredSince}
        signals={props.signals}
        signalsUnreadable={props.signalsUnreadable}
        runs={props.runs}
        marketCell={props.marketCell}
        emotionalJobs={props.emotionalJobs}
        cellGaps={props.cellGaps}
        jobGaps={props.jobGaps}
        askAnswers={props.askAnswers}
        gddHref={props.gddHref}
      />
      <WhatChanged
        memory={props.memory}
        hasVisitorNote={props.hasVisitorNote}
        revision={props.revision}
      />
      <ToolStrip
        signalsConnected={props.signals !== null}
        signalsUnreadable={props.signalsUnreadable}
      />
    </div>
  );
}
