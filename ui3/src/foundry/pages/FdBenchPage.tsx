import EmptyState from "../../components/EmptyState";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdVerdictPill from "../components/FdVerdictPill";
import "./fdbench.css";

/** A loopback realm means the run touched a local copy of the scene, never the
 *  deployed World. Matches the realm's HOST, not any substring: a deployed World
 *  such as "localhost-games.dcl.eth" or "notlocalhost.example" is not local, and
 *  a loopback like "http://[::1]:8000" or "127.0.0.1:8000" is. */
export function isLocalRealm(realm: string | null | undefined): boolean {
  if (!realm) return false;
  let host = realm.trim();
  try {
    // Parse as a URL to isolate the host. A realm without a scheme (e.g.
    // "127.0.0.1:8000") is not URL-shaped, so hand the parser one.
    host = new URL(
      /^[a-z][a-z0-9+.-]*:\/\//i.test(host) ? host : `http://${host}`,
    ).hostname;
  } catch {
    host = host.replace(/:\d+$/, "");
  }
  host = host.replace(/^\[|\]$/g, "").toLowerCase();
  return (
    host === "localhost" ||
    host === "0.0.0.0" ||
    host === "::1" ||
    /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(host)
  );
}

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

/** UTC, formatted by hand so server render and hydration agree. */
export function benchStamp(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}, ${d.getUTCFullYear()} ${pad(
    d.getUTCHours(),
  )}:${pad(d.getUTCMinutes())} UTC`;
}

export type FdBenchReportVM = {
  id: string;
  slug: string;
  runner?: "dclbots" | "arena" | null;
  realm?: string | null;
  ranAt: string;
  verdict: "pass" | "fail" | null;
  checksTotal: number | null;
  checksFailed: number | null;
  missingTools: readonly string[];
  stubbedTools: readonly string[];
  networkWrites: number | null;
  shots: number;
  /** Non-leaking evidence label (basename + short hash), computed server-side by
   *  the loader from the raw path so the absolute host path never reaches here. */
  evidence: string | null;
  replayHref: string | null;
  gameHref: string | null;
};

export function checksLabel(report: FdBenchReportVM): string {
  if (report.checksTotal === null) {
    // An arena run carries a verdict from the runner's exit code but no
    // per-check breakdown; saying "verdict not recorded" next to a PASS pill
    // would contradict the pill.
    return report.verdict === null
      ? "snapshot only — verdict not recorded"
      : "verdict from the runner's exit status — no per-check breakdown recorded";
  }
  const failed = report.checksFailed ?? 0;
  return `${report.checksTotal - failed} of ${report.checksTotal} checks passed`;
}

export function FdBenchReportCard({
  report,
  onOpen,
}: {
  report: FdBenchReportVM;
  onOpen?: (report: FdBenchReportVM) => void;
}) {
  const tools = [
    ...report.missingTools.map((t) => ({ key: `missing:${t}`, label: `missing ${t}` })),
    ...report.stubbedTools.map((t) => ({ key: `stubbed:${t}`, label: `stubbed ${t}` })),
  ];

  return (
    <article className="fd-benchcard">
      <header className="fd-benchcard__head">
        <div>
          <h3 className="fd-benchcard__slug">
            {report.gameHref ? <a href={report.gameHref}>{report.slug}</a> : report.slug}
          </h3>
          <p className="fd-benchcard__at fd-mono">{benchStamp(report.ranAt)}</p>
        </div>
        {report.runner === "arena" ? (
          <span className="fd-chip">sandbox simulation</span>
        ) : report.verdict ? (
          <FdVerdictPill verdict={report.verdict} />
        ) : (
          <span className="fd-chip">verdict not recorded</span>
        )}
      </header>

      {report.runner === "arena" ? (
        <p className="fd-benchcard__scope">
          Sandbox simulation — not a run against this World. The verdict is the
          runner&apos;s process exit status, not a test of the game.
        </p>
      ) : isLocalRealm(report.realm) ? (
        <p className="fd-benchcard__scope">
          Recorded against a local copy of this scene
          {report.realm ? ` (${report.realm})` : ""} — not the deployed World.
        </p>
      ) : null}

      <p className="fd-benchcard__checks">{checksLabel(report)}</p>

      <dl className="fd-benchcard__facts">
        <div>
          <dt>Network writes</dt>
          <dd>{report.networkWrites === null ? "not reported" : report.networkWrites}</dd>
        </div>
        <div>
          <dt>Screenshots</dt>
          <dd>{report.shots === 0 ? "none" : report.shots}</dd>
        </div>
      </dl>

      {tools.length > 0 ? (
        <p className="fd-benchcard__tools">
          {tools.map((tool) => (
            <span key={tool.key} className="fd-chip fd-chip--mono">
              {tool.label}
            </span>
          ))}
        </p>
      ) : null}

      {report.evidence ? (
        <p className="fd-benchcard__evidence fd-mono">
          evidence: {report.evidence}
        </p>
      ) : null}

      {report.replayHref ? (
        <p className="fd-benchcard__links">
          <a
            className="fd-benchcard__link"
            href={report.replayHref}
            onClick={() => onOpen?.(report)}
          >
            Open the event log
          </a>
        </p>
      ) : null}
    </article>
  );
}

export type FdBenchPageProps = {
  reports: readonly FdBenchReportVM[];
  /** Total recorded runs; the list above is capped to the most recent. */
  reportsTotal?: number;
  onReportOpen?: (report: FdBenchReportVM) => void;
};

export default function FdBenchPage({
  reports,
  reportsTotal,
  onReportOpen,
}: FdBenchPageProps) {
  const shown = reports.length;
  const total = reportsTotal ?? shown;
  return (
    <div className="fd-page fd-stack fd-bench">
      <FdPageHead
        title="Bot bench"
        intro="Runs come from the dcl-scene-bots harness — a scene-level MCP surface, no UI clicks. Its core rule is ours too: a check that cannot be evaluated fails; a run that never happened is shown as nothing."
      />

      <FdSection title="Recorded runs">
        {reports.length === 0 ? (
          <EmptyState
            variant="inline"
            title="No recorded bot runs."
            subtitle="Runs are executed by the dcl-scene-bots harness and ingested with foundry:ingest-bench."
          />
        ) : (
          <>
            {total > shown ? (
              <p className="fd-note">
                Showing the {shown} most recent of {total} runs.
              </p>
            ) : null}
            <div className="fd-bench__list">
              {reports.map((report) => (
                <FdBenchReportCard key={report.id} report={report} onOpen={onReportOpen} />
              ))}
            </div>
          </>
        )}
        <p className="fd-note">
          Every report links its full event log. Verdicts are parsed from the runner&apos;s
          own output; a snapshot without a recorded verdict says so.
        </p>
      </FdSection>

    </div>
  );
}
