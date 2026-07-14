import FdIdeaCard from "../components/FdIdeaCard";
import FdProgramChecks, {
  type FdProgramCheckVM,
} from "../components/FdProgramChecks";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import "../components/fdstat.css";
import "./foundryhome.css";

export type FoundryHomeStats = {
  scenes: number;
  gddDocs: number;
  benchRuns: number;
  lastBenchAt: string | null;
  copilotOnline: boolean;
  tokens: number;
};

export type FoundryRailCard = {
  id: string;
  num: string;
  name: string;
  oneLiner: string;
  surfaceLabel: string;
  surfaceHref: string;
};

export const FOUNDRY_RAIL: readonly FoundryRailCard[] = [
  {
    id: "games",
    num: "01",
    name: "Games",
    oneLiner:
      "Real games deployed to Decentraland Worlds. Dates come from the deployment entities, not from us.",
    surfaceLabel: "Play",
    surfaceHref: "/foundry/play",
  },
  {
    id: "gdd",
    num: "02",
    name: "Design docs",
    oneLiner:
      "shortGDDs in the Creator Success format. Open questions stay marked TBD instead of being papered over.",
    surfaceLabel: "Design docs",
    surfaceHref: "/foundry/gdd",
  },
  {
    id: "copilot",
    num: "03",
    name: "Copilot",
    oneLiner:
      "A self-hosted opencode instance wired to our own model gateway. Every token it spends is counted and shown.",
    surfaceLabel: "Copilot",
    surfaceHref: "/foundry/copilot",
  },
  {
    id: "bench",
    num: "04",
    name: "Bench",
    oneLiner:
      "Bots test the games through the dcl-scene-bots harness. A check that cannot be evaluated fails.",
    surfaceLabel: "Console → Bench",
    surfaceHref: "/foundry/console/bench",
  },
  {
    id: "trajectories",
    num: "05",
    name: "Trajectories",
    oneLiner:
      "Each run is an append-only event log. Scrub back through any episode.",
    surfaceLabel: "Console → Trajectories",
    surfaceHref: "/foundry/console/trajectories",
  },
  {
    id: "costs",
    num: "06",
    name: "Costs",
    oneLiner:
      "Token counts are measured; the dollar figure is labeled reference pricing.",
    surfaceLabel: "Console → Costs",
    surfaceHref: "/foundry/console/costs",
  },
  {
    id: "exchange",
    num: "07",
    name: "Exchange",
    oneLiner:
      "Ask for what you want built; pledge on others' requests. Counts are pure row counts.",
    surfaceLabel: "Exchange",
    surfaceHref: "/foundry/exchange",
  },
];

const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

// UTC, formatted by hand: the same string has to come out of the server render and
// the client hydration, and Intl's output moves with the runtime's ICU build.
function stamp(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}, ${pad(d.getUTCHours())}:${pad(
    d.getUTCMinutes(),
  )} UTC`;
}

function group(n: number): string {
  const digits = String(Math.trunc(Math.abs(n)));
  let out = "";
  for (let i = 0; i < digits.length; i += 1) {
    if (i > 0 && (digits.length - i) % 3 === 0) out += ",";
    out += digits[i];
  }
  return n < 0 ? `-${out}` : out;
}

export type FoundryHomeDeploy = {
  count: number;
  range: string | null;
};

export type FoundryHomeProps = {
  stats: FoundryHomeStats;
  checks: readonly FdProgramCheckVM[];
  /** Count and date range of the deployed games, derived from the scene rows. */
  deploy?: FoundryHomeDeploy;
  expandedIdea: string | null;
  onIdeaToggle: (ideaId: string) => void;
  rail?: readonly FoundryRailCard[];
};

export default function FoundryHome({
  stats,
  checks,
  deploy,
  expandedIdea,
  onIdeaToggle,
  rail = FOUNDRY_RAIL,
}: FoundryHomeProps) {
  // "Games registered" counts every scene row; the lede counts only the ones with
  // a Worlds deployment. Same data, two framings — so the note reconciles them
  // (8 = 7 deployed + 1 repo starter) instead of leaving a bare 8 next to "7 games".
  const deployedCount = deploy?.count ?? 0;
  const templateCount = Math.max(0, stats.scenes - deployedCount);
  const scenesNote =
    templateCount > 0
      ? `${group(deployedCount)} deployed to Decentraland Worlds, plus ${group(
          templateCount,
        )} starter scene${templateCount === 1 ? "" : "s"} that live${
          templateCount === 1 ? "s" : ""
        } only in this repo.`
      : "Imported from the Decentraland Worlds mirror by foundry:import-real.";
  return (
    <div className="fd-page fd-stack fd-home">
      <FdPageHead
        title="The Foundry"
        intro="An open build bench for Decentraland games. Real games, real tests, real costs."
      />

      {/* Count and date window come from the loaded scene rows, never typed in —
          a redeploy or an eighth game moves the sentence. With no rows there is
          nothing to count, so the lede asserts nothing about games rather than
          inventing them. */}
      <p className="fd-home__lede">
        {deploy && deploy.count > 0 && deploy.range ? (
          <>
            {deploy.count} game{deploy.count === 1 ? "" : "s"}, deployed to
            Decentraland Worlds{" "}
            {deploy.range.includes(" and ") ? "between" : "in"} {deploy.range},{" "}
            {deploy.count === 1 ? "anchors" : "anchor"} the bench: some carry
            local-copy bench runs from a scene-level test harness, design docs
            document what each game promises, and a self-hosted copilot helps
            draft the next ones. Every number on this site traces to a deployment
            entity, a git history, a bot run, or a token count — and where there
            is no data yet, the page says so.
          </>
        ) : (
          <>
            No games have been imported yet — when they are, this page counts them
            from their deployment entities and this line follows the data. Every
            number on this site traces to a deployment entity, a git history, a
            bot run, or a token count — and where there is no data yet, the page
            says so.
          </>
        )}
      </p>

      <p className="fd-home__enter">
        New here? Pick a door — play, build or host — and walk in. All three open
        on the same Decentraland.
      </p>

      <div className="fd-home__ctas">
        <a className="fd-home__cta fd-home__cta--primary" href="/foundry/select">
          Choose how you start
        </a>
        <a className="fd-home__cta fd-home__cta--secondary" href="/foundry/play">
          Browse the games
        </a>
        <a className="fd-home__cta fd-home__cta--secondary" href="/foundry/gdd">
          Read the design docs
        </a>
        <a className="fd-home__cta fd-home__cta--ghost" href="/foundry/console/bench">
          Open the console
        </a>
      </div>

      <FdSection title="Right now">
        <div className="fd-statrow">
          <FdStat
            label="Games registered"
            value={group(stats.scenes)}
            note={scenesNote}
          />
          <FdStat
            label="Design docs"
            value={group(stats.gddDocs)}
            note="shortGDDs, with their own honesty markers parsed into the page."
          />
          <FdStat
            label="Bench runs recorded"
            value={group(stats.benchRuns)}
            note={
              stats.lastBenchAt
                ? `Most recent run ${stamp(stats.lastBenchAt)}.`
                : "No run has been ingested yet."
            }
          />
          <FdStat
            label="Copilot"
            value={stats.copilotOnline ? "online" : "offline"}
            note="Probed from this server when the page rendered."
          />
          <FdStat
            label="Copilot tokens"
            value={group(stats.tokens)}
            note="Measured per message. The dollar figure lives on the costs page, labeled."
          />
        </div>
      </FdSection>

      <FdSection
        title="Program checks"
        sub="Each reading is a live count over a real table, with the sentence that says where it came from. No trends and no verdicts — history has to accrue first."
      >
        <FdProgramChecks checks={checks} />
      </FdSection>

      <FdSection title="What is here">
        <div className="fd-home__ideas">
          {rail.map((card) => (
            <FdIdeaCard
              key={card.id}
              num={card.num}
              name={card.name}
              oneLiner={card.oneLiner}
              surfaceLabel={card.surfaceLabel}
              surfaceHref={card.surfaceHref}
              expanded={expandedIdea === card.id}
              onToggle={() => onIdeaToggle(card.id)}
            />
          ))}
        </div>
      </FdSection>

    </div>
  );
}
