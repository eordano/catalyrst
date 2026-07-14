import type { ReactNode } from "react";
import Button from "../../atoms/Button";
import FdEmbedViewport from "../components/FdEmbedViewport";
import { JOB_NAMES, jobTitle, type FdEmotionalJobVM } from "../components/FdEmotionalJobs";
import FdHistorySpine, {
  type FdHistoryFutureVM,
  type FdHistoryNodeKind,
  type FdHistoryNodeVM,
  type FdHistoryReadingVM,
} from "../components/FdHistorySpine";
import type { FdStewardVM } from "../components/FdContinuity";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import FdVerdictPill, { runVerdictReading } from "../components/FdVerdictPill";
import {
  formatSize,
  humanizeEntityTitle,
  isTemplateDescription,
  marketCellName,
  marketCellTitle,
  mirrorProvenance,
  parcelLabel,
  type FdMarketCellVM,
} from "../components/FdGameCard";
import { dayUTC, groupDigits, plural, stampUTC } from "../fmt";
import { FdBenchReportCard, type FdBenchReportVM } from "./FdBenchPage";
import "./fdgamedetail.css";

export type FdGameDetailVM = {
  slug: string;
  title: string;
  worldName: string | null;
  deployedAt: string | null;
  /** When these deployment facts were read from the worlds mirror. */
  importedAt?: string | null;
  sizeBytes: number | null;
  parcels: number | null;
  /** The row's citation; it rides the World fact's provenance title. */
  sourceNote: string;
  /** The deployment entity's own scene.json display facts, when imported. */
  description?: string | null;
  thumbnailUrl?: string | null;
  /** Public content-server URL of the deployment entity cited in sourceNote. */
  entityHref?: string | null;
  entityId?: string | null;
  /** Non-leaking labels (basename + short hash), computed server-side by the
   *  loader from the raw paths so no absolute host path reaches the client. */
  repoLabel: string | null;
  botManifestLabel: string | null;
};

export type FdGameEmbedVM = {
  url: string;
  reachable: boolean;
  probedAt: string;
  /** The exact content URL probed, and the status seen (null on timeout). */
  probedUrl?: string;
  status?: number | null;
};

export type FdGameLinkTarget = "play" | "editor" | "world";

/** One upcoming session occurrence that names this game, from the sessions DB. */
export type FdGameGatheringVM = {
  seriesId: string;
  title: string;
  occurrenceAt: string;
  rsvpCount: number;
};

/** One version of a truly-linked design doc chain, stored stats only. */
export type FdGameDesignVersionVM = {
  id: string;
  version: number;
  createdAt: string;
  /** All four honesty markers summed — 0 genuinely means none remain. */
  markers: number;
};

/** This program's dated same-concept reading naming this game — adjacency,
 *  never an implementation claim. */
export type FdGameConceptReadingVM = {
  docId: string;
  docTitle: string;
  readAt: string;
  rationale: string;
  confidence: string;
  /** Set when the reading named an older version of the linked doc's chain —
   *  the judgment predates the edits that minted the head. */
  readAgainstVersion?: number | null;
};

export type FdGameChangelogVM = {
  at: string;
  origin: "import" | "visitor";
  note: string;
  sourceNote: string;
};

/** One recorded run on this scene, verdict joined from its bot report. */
export type FdGameRunVM = {
  id: string;
  provenance: "bot" | "visitor";
  runner: string | null;
  createdAt: string;
  events: number;
  finishKind: string | null;
  verdict: "pass" | "fail" | null;
  checksFailed: number | null;
  checksTotal: number | null;
  checksUnevaluable: number | null;
};

/** A bench report whose run recorded no trajectory — its own label shape. */
export type FdGameBenchOnlyRunVM = {
  id: string;
  ranAt: string;
  checksFailed: number | null;
  checksTotal: number | null;
  checksUnevaluable: number | null;
};

export type FdGameCheckVM = {
  kind: string;
  /** The check's own stated purpose — the goal, not a grade. */
  why: string;
  state: "passed" | "failed" | "blocked";
  detail: string;
};

export type FdGameDemandAskVM = {
  requestId: string;
  title: string;
  pledges: number;
};

export type FdGameBenchSummaryVM = {
  runs: number;
  sandboxRuns: number;
  lastVerdict: "pass" | "fail" | null;
  lastChecksFailed: number | null;
  lastChecksTotal: number | null;
  /** Failed checks the harness itself could not evaluate — see runVerdictReading. */
  lastChecksUnevaluable: number | null;
  lastRealRanAt: string | null;
};

export type FdGameDetailProps = {
  game: FdGameDetailVM;
  embed: FdGameEmbedVM | null;
  worldHref: string | null;
  editorHref: string;
  /** The truly-linked design doc, when the registry links one. */
  gddHref: string | null;
  /** The earliest ACTIVE steward — the person standing behind this game.
   *  Null renders the claim affordance instead of a byline. */
  steward?: FdStewardVM | null;
  /** The named checks of the latest real run — the goals behind "N of M". */
  checklist?: { ranAt: string; rows: readonly FdGameCheckVM[] } | null;
  /** Asks this program read as answered by this game, with real pledge counts. */
  demandAsks?: readonly FdGameDemandAskVM[];
  /** This program's market-cell reading of this game. null/absent = the game
   *  has not been read at all — distinct from a row whose cell is null (read,
   *  and honestly unclassifiable). */
  marketCell?: FdMarketCellVM | null;
  /** This program's emotional-job reading; job rows become head chips. */
  emotionalJobs?: readonly FdEmotionalJobVM[] | null;
  designVersions?: readonly FdGameDesignVersionVM[];
  conceptReadings?: readonly FdGameConceptReadingVM[];
  changelog?: readonly FdGameChangelogVM[];
  runs?: readonly FdGameRunVM[];
  runsTotal?: number;
  benchOnlyRuns?: readonly FdGameBenchOnlyRunVM[];
  benchSummary?: FdGameBenchSummaryVM | null;
  todayIso: string;
  reports: readonly FdBenchReportVM[];
  /** Upcoming sessions on the community calendar that name this game. */
  gatherings?: readonly FdGameGatheringVM[];
  embedStarted: boolean;
  onEmbedStart: () => void;
  onLinkOpen: (target: FdGameLinkTarget) => void;
  onReportOpen?: (report: FdBenchReportVM) => void;
  onHistoryNodeOpen?: (kind: FdHistoryNodeKind | "upcoming" | "reading") => void;
  /** The continuity section (memory, stewards, transfers). Absent renders exactly
   *  today's page. */
  continuity?: ReactNode;
  /** The game's response page — who came, what mattered, what changed. */
  responseHref?: string | null;
};

/** Enough of the entity hash to recognise it; the full id rides the title and
 *  an sr-only twin, so nothing is lost to a reader who cannot hover. */
function shortEntityId(id: string): string {
  return id.length > 14 ? `${id.slice(0, 14)}…` : id;
}

function Fact({
  label,
  prov,
  mono = false,
  children,
}: {
  label: string;
  prov?: string | null;
  mono?: boolean;
  children: ReactNode;
}) {
  return (
    <div>
      <dt>{label}</dt>
      <dd title={prov ?? undefined}>
        {mono ? <span className="fd-mono">{children}</span> : children}
        {prov ? <span className="u-sr-only"> — {prov}</span> : null}
      </dd>
    </div>
  );
}

export default function FdGameDetail({
  game,
  embed,
  worldHref,
  editorHref,
  gddHref,
  steward = null,
  checklist = null,
  demandAsks = [],
  marketCell = null,
  emotionalJobs = null,
  designVersions = [],
  conceptReadings = [],
  changelog = [],
  runs = [],
  runsTotal = 0,
  benchOnlyRuns = [],
  benchSummary = null,
  todayIso,
  reports,
  gatherings = [],
  embedStarted,
  onEmbedStart,
  onLinkOpen,
  onReportOpen,
  onHistoryNodeOpen,
  continuity,
  responseHref = null,
}: FdGameDetailProps) {
  const deployed = dayUTC(game.deployedAt);
  const size = formatSize(game.sizeBytes);
  const prov = mirrorProvenance(game.importedAt);
  const jobReads = (emotionalJobs ?? []).filter(
    (r): r is FdEmotionalJobVM & { job: NonNullable<FdEmotionalJobVM["job"]> } =>
      r.job !== null,
  );

  const verdictReading = benchSummary?.lastVerdict
    ? runVerdictReading({
        verdict: benchSummary.lastVerdict,
        checksFailed: benchSummary.lastChecksFailed,
        checksTotal: benchSummary.lastChecksTotal,
        checksUnevaluable: benchSummary.lastChecksUnevaluable,
      })
    : null;
  const verdictTitle =
    verdictReading?.detail != null
      ? verdictReading.detail +
        (benchSummary?.lastRealRanAt
          ? ` on ${dayUTC(benchSummary.lastRealRanAt)}`
          : "")
      : undefined;

  const nodes: FdHistoryNodeVM[] = [];
  if (designVersions.length > 0) {
    for (const v of designVersions) {
      nodes.push({
        key: `design-${v.id}`,
        kind: "designed",
        at: v.createdAt,
        href: `/foundry/gdd/${v.id}`,
        label: `Design v${v.version} — ${
          v.markers === 0 ? "no markers" : plural(v.markers, "marker")
        }`,
      });
    }
  } else {
    nodes.push({
      key: "design-none",
      kind: "designed",
      at: null,
      href: "/foundry/gdd",
      prefix: "No design doc linked.",
      label: "The design docs",
      muted: true,
    });
  }

  // The import writes the deployment as a changelog row stamped with the
  // deployment's own instant; the live node below is that same fact, with the
  // size and parcels attached. One deployment, one node.
  const deployedInstant = game.deployedAt ? Date.parse(game.deployedAt) : null;
  changelog.forEach((c, i) => {
    if (
      c.origin === "import" &&
      deployedInstant !== null &&
      Date.parse(c.at) === deployedInstant
    ) {
      return;
    }
    nodes.push({
      key: `built-${i}-${c.at}`,
      kind: "built",
      at: c.at,
      href: "#memory",
      label: c.note,
      title: c.sourceNote,
      chip: { text: c.origin },
    });
  });

  if (game.deployedAt) {
    const facts = [size, game.parcels === null ? null : parcelLabel(game.parcels)]
      .filter(Boolean)
      .join(", ");
    nodes.push({
      key: "live",
      kind: "live",
      at: game.deployedAt,
      href: game.entityHref ?? null,
      label:
        `Deployed to ${game.worldName ?? "Decentraland Worlds"}` +
        (facts ? ` — ${facts}` : ""),
    });
  }
  for (const r of runs) {
    const reading =
      r.runner !== "arena" && r.verdict
        ? runVerdictReading({
            verdict: r.verdict,
            checksFailed: r.checksFailed,
            checksTotal: r.checksTotal,
            checksUnevaluable: r.checksUnevaluable,
          })
        : null;
    nodes.push({
      key: `run-${r.id}`,
      kind: "played",
      at: r.createdAt,
      href: `/foundry/console/trajectories/${r.id}`,
      label: `${r.provenance} run · ${plural(r.events, "event")} · ${
        r.finishKind ?? "never closed its turn"
      }`,
      // The arena-row rule: the raw runner value rides the chip's title, never
      // the prose; a sandbox exit code is not a verdict on the game.
      chip: r.runner === "arena" ? { text: "sandbox", title: "runner: arena" } : null,
      verdict: reading?.verdict ?? null,
      verdictLabel: reading?.label ?? null,
      verdictTitle: reading?.detail ?? null,
    });
  }
  for (const b of benchOnlyRuns) {
    nodes.push({
      key: `bench-${b.id}`,
      kind: "played",
      at: b.ranAt,
      href: `/foundry/console/evidence/${b.id}`,
      label:
        b.checksTotal === null
          ? "bot run · verdict not recorded"
          : (b.checksFailed ?? 0) > 0
            ? `bot run · ${runVerdictReading({
                verdict: "fail",
                checksFailed: b.checksFailed,
                checksTotal: b.checksTotal,
                checksUnevaluable: b.checksUnevaluable,
              }).label}`
            : `bot run · all ${plural(b.checksTotal, "check")} passed`,
    });
  }
  nodes.sort(
    (a, b) =>
      (a.at ? Date.parse(a.at) : Number.NEGATIVE_INFINITY) -
      (b.at ? Date.parse(b.at) : Number.NEGATIVE_INFINITY),
  );

  // A reading is this program's judgment about the game, not something the game
  // did — it sits beside the spine rather than on its time axis.
  const readings: FdHistoryReadingVM[] =
    designVersions.length > 0
      ? []
      : conceptReadings.map((r) => ({
          key: `reading-${r.docId}`,
          href: `/foundry/gdd/${r.docId}`,
          label: r.docTitle,
          title:
            `This program's reading, ${dayUTC(r.readAt)}` +
            (r.readAgainstVersion != null
              ? `, read against v${r.readAgainstVersion}`
              : "") +
            ` — ${r.rationale} (${r.confidence})`,
        }));

  const futures: FdHistoryFutureVM[] = gatherings.map((g) => ({
    key: `${g.seriesId}#${g.occurrenceAt}`,
    at: g.occurrenceAt,
    label: `${g.title} · ${plural(g.rsvpCount, "person")} coming`,
    href: "/foundry/sessions",
  }));

  return (
    <div className="fd-page fd-stack fd-gamedetail">
      <FdPageHead
        eyebrow="Games"
        title={humanizeEntityTitle(game.title)}
        intro={
          game.description && !isTemplateDescription(game.description)
            ? game.description
            : undefined
        }
        crumbs={
          <>
            <a href="/foundry/play">← All games</a>
            {gddHref ? (
              <>
                {" · "}
                <a href={gddHref}>Design doc</a>
              </>
            ) : null}
            {responseHref ? (
              <>
                {" · "}
                <a href={responseHref}>Response</a>
              </>
            ) : null}
          </>
        }
        aside={
          <div className="fd-gamedetail__aside">
            {game.thumbnailUrl ? (
              <img
                className="fd-gamedetail__thumb"
                src={game.thumbnailUrl}
                alt=""
                loading="lazy"
              />
            ) : null}
            <p className="fd-gamedetail__headchips">
              {steward ? (
                <a
                  className="fd-chip fd-gamedetail__steward"
                  href="#memory"
                  title={`since ${dayUTC(steward.since)} — “${steward.basis}”`}
                >
                  Stewarded by{" "}
                  {"name" in steward.actor ? steward.actor.name : steward.actor.badge}
                  <span className="u-sr-only">
                    {" "}
                    — since {dayUTC(steward.since)}: {steward.basis}
                  </span>
                </a>
              ) : (
                <a
                  className="fd-chip fd-gamedetail__steward"
                  href="#memory"
                  title="stewardship is claimed in the continuity section below, in your own words"
                >
                  No steward — claim it
                </a>
              )}
              {demandAsks.map((a) =>
                a.pledges > 0 ? (
                  <a
                    key={a.requestId}
                    className="fd-chip"
                    href={`/foundry/exchange/${a.requestId}`}
                    title={`real pledges on “${a.title}” — the ask this program read as answered by this game`}
                  >
                    {a.pledges} pledged to show up
                  </a>
                ) : null,
              )}
              {verdictReading ? (
                <a
                  className="fd-gamedetail__verdictlink"
                  href="#bench"
                  title={verdictTitle}
                  aria-label={
                    verdictTitle
                      ? `${verdictReading.label} — ${verdictTitle}`
                      : `last run ${verdictReading.label}`
                  }
                >
                  <FdVerdictPill
                    verdict={verdictReading.verdict}
                    label={verdictReading.label}
                  />
                </a>
              ) : null}
              {marketCell ? (
                <span className="fd-cellchip" title={marketCellTitle(marketCell)}>
                  {marketCellName(marketCell.cell)} · {dayUTC(marketCell.classifiedAt)}
                  <span className="u-sr-only"> — {marketCellTitle(marketCell)}</span>
                </span>
              ) : null}
              {jobReads.map((r) => (
                <span key={r.job} className="fd-cellchip" title={jobTitle(r)}>
                  {JOB_NAMES[r.job]} · {dayUTC(r.readAt)}
                  <span className="u-sr-only"> — {jobTitle(r)}</span>
                </span>
              ))}
            </p>
          </div>
        }
      />

      <FdSection title="The build">
        <dl className="fd-facts">
          <Fact label="Deployed" prov={deployed ? prov : null}>
            {deployed ?? "—"}
          </Fact>
          <Fact label="Size" prov={size ? prov : null}>
            {size ?? "—"}
          </Fact>
          <Fact label="Parcels" prov={game.parcels === null ? null : prov}>
            {game.parcels === null ? "—" : groupDigits(game.parcels)}
          </Fact>
          <Fact label="World" prov={game.worldName ? game.sourceNote : null} mono>
            {game.worldName ?? "—"}
          </Fact>
          {game.entityId && game.entityHref ? (
            <Fact label="Entity" prov={game.entityId} mono>
              <a href={game.entityHref} rel="noreferrer">
                {shortEntityId(game.entityId)}
              </a>
            </Fact>
          ) : null}
          {game.repoLabel ? (
            <Fact label="Harness copy" prov="content hash of the stored copy" mono>
              {game.repoLabel}
            </Fact>
          ) : null}
          {game.botManifestLabel ? (
            <Fact label="Bot manifest" prov="content hash of the stored copy" mono>
              {game.botManifestLabel}
            </Fact>
          ) : null}
        </dl>
      </FdSection>

      <FdSection title="Play it">
        {embed === null ? (
          <p className="fd-empty">
            An SDK7 starter scene in this repo — no world is deployed to load.
          </p>
        ) : embed.reachable ? (
          <FdEmbedViewport
            url={embed.url}
            title={`${game.title} in the Decentraland client`}
            weight={size}
            started={embedStarted}
            onStart={onEmbedStart}
          />
        ) : (
          <p className="fd-note">
            Probed {embed.probedUrl ?? "the world's content"} at{" "}
            <FdTime iso={embed.probedAt}>{stampUTC(embed.probedAt)}</FdTime> —{" "}
            {embed.status ? `HTTP ${embed.status}` : "no response"}. The link below
            still opens the client.
          </p>
        )}

        <div className="fd-gamedetail__links">
          {worldHref ? (
            <Button
              as="a"
              variant="primary"
              size="sm"
              href={worldHref}
              target="_blank"
              rel="noreferrer"
              onClick={() => onLinkOpen("world")}
            >
              Open in the Decentraland client
            </Button>
          ) : null}
          <Button
            as="a"
            variant="ghost"
            size="sm"
            href={editorHref}
            onClick={() => onLinkOpen("editor")}
          >
            Start a new scene in the editor
          </Button>
        </div>
      </FdSection>

      <FdSection title="History">
        <FdHistorySpine
          nodes={nodes}
          todayIso={todayIso}
          futures={futures}
          readings={readings}
          allRunsHref={
            runsTotal > 0
              ? `/foundry/console/trajectories?scene=${encodeURIComponent(game.slug)}`
              : null
          }
          onNodeOpen={onHistoryNodeOpen}
        />
      </FdSection>

      <FdSection
        id="bench"
        title="Runs"
        badge={
          benchSummary && benchSummary.runs > 0 ? (
            <span
              className="fd-chip fd-num"
              title="stored bench runs, sandbox sims excluded"
            >
              {benchSummary.runs}
            </span>
          ) : undefined
        }
        aside={
          benchSummary && benchSummary.sandboxRuns > 0 ? (
            <span className="fd-chip" title="ran in the sandbox, not against this World">
              {plural(benchSummary.sandboxRuns, "sandbox run")}
            </span>
          ) : undefined
        }
      >
        {checklist ? (
          <div className="fd-gamedetail__checks">
            <h3 className="fd-subhead">
              The checks — run of {dayUTC(checklist.ranAt)}
            </h3>
            <ul className="fd-gamedetail__checklist">
              {checklist.rows.map((c, i) => (
                <li key={i} className={"is-" + c.state} title={c.detail}>
                  <span className="fd-gamedetail__checkstate">
                    {c.state === "passed" ? "passed" : c.state === "blocked" ? "blocked" : "failed"}
                  </span>{" "}
                  {c.why || <span className="fd-mono">{c.kind}</span>}
                  <span className="u-sr-only"> — {c.detail}</span>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
        {reports.length === 0 ? (
          <p className="fd-empty">No recorded bot run for this game.</p>
        ) : (
          <div className="fd-gamedetail__reports">
            {reports.map((report) => (
              <FdBenchReportCard key={report.id} report={report} onOpen={onReportOpen} />
            ))}
          </div>
        )}
      </FdSection>

      {continuity ? (
        <div className="fd-stack" id="memory">
          {continuity}
        </div>
      ) : null}
    </div>
  );
}
