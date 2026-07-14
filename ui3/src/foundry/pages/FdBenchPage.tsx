import { useState } from "react";
import { plural, stampUTC } from "../fmt";
import FdSection, { FD_UNAVAILABLE, FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import FdTime from "../components/FdTime";
import FdVerdictPill, { runVerdictReading } from "../components/FdVerdictPill";
import "../components/fdstat.css";
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

export { stampUTC as benchStamp };

/** A distinct (scene, realm) pair a real (non-arena) bench run touched, read
 *  from the stored reports — the datum behind every "local copies vs deployed
 *  Worlds" sentence. */
export type FdBenchTargetVM = {
  sceneId: string | null;
  realm: string | null;
};

export type FdBenchTargetTally = {
  /** Distinct scenes with a run against a non-loopback realm. */
  world: number;
  /** Distinct scenes with a run against a loopback realm (a local copy). */
  local: number;
  /** Distinct scenes whose runs recorded no realm at all. */
  unrecorded: number;
};

export function tallyBenchTargets(
  targets: readonly FdBenchTargetVM[],
): FdBenchTargetTally {
  const world = new Set<string>();
  const local = new Set<string>();
  const unrecorded = new Set<string>();
  for (const t of targets) {
    const key = t.sceneId ?? `?${t.realm ?? ""}`;
    if (t.realm === null) unrecorded.add(key);
    else if (isLocalRealm(t.realm)) local.add(key);
    else world.add(key);
  }
  return { world: world.size, local: local.size, unrecorded: unrecorded.size };
}

/**
 * The one sentence about what the bench runs actually touched, derived from
 * the rows rather than typed in: the day a run targets a deployed World, every
 * surface that renders this moves with it.
 */
export function benchTargetsSentence(
  targets: readonly FdBenchTargetVM[],
): string {
  const t = tallyBenchTargets(targets);
  if (targets.length === 0) return "no bot bench run has been ingested yet";
  if (t.world > 0) {
    const w = `${t.world} scene${t.world === 1 ? " has" : "s have"} bench runs against ${
      t.world === 1 ? "its" : "their"
    } deployed World${t.world === 1 ? "" : "s"}`;
    return t.local > 0
      ? `${w}, and ${t.local} against local copies`
      : w;
  }
  if (t.local > 0) {
    return `bench runs so far exercised local copies of ${t.local} scene${
      t.local === 1 ? "" : "s"
    }, not the deployed Worlds`;
  }
  return "bench runs are recorded, but the harness did not record their targets";
}

export type FdBenchReportVM = {
  id: string;
  slug: string;
  /** The game's own name, when the run's scene resolves to a registry row. The
   *  slug stays visible as the stored identifier. */
  title?: string | null;
  runner?: "dclbots" | "arena" | null;
  realm?: string | null;
  ranAt: string;
  verdict: "pass" | "fail" | null;
  checksTotal: number | null;
  checksFailed: number | null;
  /** Failed checks whose stored verdict says "cannot evaluate" — counted as
   *  failed by policy; the label says so when they are the only failures. */
  checksUnevaluable?: number;
  missingTools: readonly string[];
  stubbedTools: readonly string[];
  networkWrites: number | null;
  shots: number;
  /** Non-leaking evidence label (basename + short hash), computed server-side by
   *  the loader from the raw path so the absolute host path never reaches here. */
  evidence: string | null;
  /** The evidence page for this run — the label links there, never to a path. */
  evidenceHref?: string | null;
  replayHref: string | null;
  gameHref: string | null;
  /** Frames sampled server-side from the surviving evidence shots, already
   *  mapped to the evidence-file route — raw paths never leave the server.
   *  Absent/empty renders no strip. */
  shotHrefs?: readonly string[];
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
  const unevaluable = report.checksUnevaluable ?? 0;
  if (failed > 0 && unevaluable === failed) {
    // Every failure is a check the harness could not evaluate. The policy that
    // makes them failures travels with the number, not just the intro prose.
    const could = `${failed} check${failed === 1 ? "" : "s"} could not be evaluated — counted as failed`;
    return failed === report.checksTotal
      ? could
      : `${report.checksTotal - failed} of ${report.checksTotal} checks passed — ${could}`;
  }
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
  const shots = report.shotHrefs ?? [];
  // Same reading as the shelf card and the game header: a run whose only
  // failures the harness could not evaluate goes neutral instead of branding
  // the game; the checks line below still spells out the stored policy.
  const reading =
    report.runner !== "arena" && report.verdict
      ? runVerdictReading({
          verdict: report.verdict,
          checksFailed: report.checksFailed,
          checksTotal: report.checksTotal,
          checksUnevaluable: report.checksUnevaluable ?? null,
        })
      : null;
  const stripHref = report.replayHref ?? report.evidenceHref ?? null;
  const [frame, setFrame] = useState(0);
  const shown = Math.min(frame, Math.max(0, shots.length - 1));
  const name = report.title ?? report.slug;
  // An absent fact is not a fact: a card shows only what the run recorded.
  const facts: { label: string; value: string | number; mono?: boolean }[] = [
    ...(report.title ? [{ label: "Scene", value: report.slug, mono: true }] : []),
    ...(report.networkWrites === null
      ? []
      : [{ label: "Network writes", value: report.networkWrites }]),
    ...(report.shots === 0 ? [] : [{ label: "Screenshots", value: report.shots }]),
  ];

  return (
    <article className="fd-benchcard">
      <header className="fd-benchcard__head">
        <div>
          <h3 className="fd-benchcard__slug">
            {report.gameHref ? <a href={report.gameHref}>{name}</a> : name}
          </h3>
          <p className="fd-benchcard__at">
            <FdTime iso={report.ranAt} title={report.ranAt} className="fd-mono">
              {stampUTC(report.ranAt)}
            </FdTime>
          </p>
        </div>
        {report.runner === "arena" ? (
          // The arena-row rule: a sandbox simulation carries no verdict pill —
          // its exit status renders as mono text, never as a judgment on the
          // game.
          <span className="fd-benchcard__sandbox">
            <span className="fd-chip" title={`runner: ${report.runner}`}>
              sandbox
              <span className="u-sr-only"> (stored value: arena)</span>
            </span>
            {report.verdict ? (
              <span
                className="fd-mono"
                title="the runner's exit status — not a verdict on the game"
              >
                exit: {report.verdict}
              </span>
            ) : null}
          </span>
        ) : reading ? (
          <span title={reading.detail ?? undefined}>
            <FdVerdictPill verdict={reading.verdict} label={reading.label} />
          </span>
        ) : (
          <span className="fd-chip">verdict not recorded</span>
        )}
      </header>

      <p className="fd-benchcard__checks">{checksLabel(report)}</p>

      {shots.length > 0 && stripHref ? (
        // One anchor: pointer-move scrubs the preview through the captured
        // frames; click and keyboard land on the full replay the frames came
        // from, so the scrub is a truthful preview of the click target.
        <a
          className="fd-benchcard__filmstrip"
          href={stripHref}
          onClick={() => onOpen?.(report)}
        >
          <img className="fd-benchcard__stripview" src={shots[shown]} alt="" />
          <span className="fd-benchcard__stripframes" aria-hidden="true">
            {shots.map((url, i) => (
              <img
                key={url}
                src={url}
                alt=""
                loading="lazy"
                className={i === shown ? "is-active" : undefined}
                onPointerEnter={() => setFrame(i)}
              />
            ))}
          </span>
          <span className="u-sr-only">
            {shots.length} captured frames — open the full run
          </span>
        </a>
      ) : null}

      {facts.length > 0 ? (
        <dl className="fd-benchcard__facts">
          {facts.map((fact) => (
            <div key={fact.label}>
              <dt>{fact.label}</dt>
              <dd className={fact.mono ? "fd-mono" : undefined}>{fact.value}</dd>
            </div>
          ))}
        </dl>
      ) : null}

      <p className="fd-benchcard__chips">
        {isLocalRealm(report.realm) ? (
          <span
            className="fd-chip"
            title={`realm ${report.realm} — a local copy of the scene, not the deployed World`}
          >
            local copy
            <span className="u-sr-only"> (realm {report.realm})</span>
          </span>
        ) : null}
        {tools.map((tool) => (
          <span key={tool.key} className="fd-chip fd-chip--mono">
            {tool.label}
          </span>
        ))}
        {report.evidence ? (
          report.evidenceHref ? (
            <a
              className="fd-chip fd-chip--mono"
              href={report.evidenceHref}
              title="the files this run wrote, as they survive on the operator host"
            >
              {report.evidence}
            </a>
          ) : (
            <span className="fd-chip fd-chip--mono">{report.evidence}</span>
          )
        ) : null}
      </p>

      {report.replayHref ? (
        <p className="fd-benchcard__links">
          {/* The whole card is this link's hit area (its ::after stretches over
              the article); the game and evidence links sit above it. */}
          <a
            className="fd-benchcard__link"
            href={report.replayHref}
            onClick={() => onOpen?.(report)}
          >
            Open the run log
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
  /** Arena sandbox simulations stored alongside — listed and labeled below,
   *  never counted as runs (the arena-row rule). */
  sandboxTotal?: number;
  /** Distinct (scene, realm) pairs real runs touched — the header words its
   *  local-copies-vs-deployed-Worlds sentence from these rows. Null when the
   *  records could not be read; the sentence is then omitted, not guessed. */
  targets?: readonly FdBenchTargetVM[] | null;
  /** The records could not be read on this deployment. */
  unavailable?: boolean;
  onReportOpen?: (report: FdBenchReportVM) => void;
};

export default function FdBenchPage({
  reports,
  reportsTotal,
  sandboxTotal,
  targets = null,
  unavailable = false,
  onReportOpen,
}: FdBenchPageProps) {
  const shown = reports.length;
  const shownSandbox = reports.filter((r) => r.runner === "arena").length;
  const runsTotal = reportsTotal ?? shown - shownSandbox;
  const simsTotal = sandboxTotal ?? shownSandbox;
  const failing = reports.filter((r) => r.verdict === "fail").length;

  if (unavailable) {
    return (
      <div className="fd-stack fd-bench">
        <FdPageHead title="Runs" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return (
    <div className="fd-stack fd-bench">
      <FdPageHead title="Runs" />

      {targets !== null ? (
        <p className="fd-note">{benchTargetsSentence(targets)}</p>
      ) : null}

      {shown > 0 ? (
        <div className="fd-statrow">
          <FdStat
            label="Runs"
            value={runsTotal}
            title="one card per harness run — sandbox simulations are not counted"
          />
          {simsTotal > 0 ? (
            <FdStat
              label="Sandbox sims"
              value={simsTotal}
              title="listed and labeled below, never counted as runs"
            />
          ) : null}
          <FdStat
            label="Failed"
            value={failing}
            title="verdicts among the runs listed below; a check that cannot be evaluated counts as failed"
          />
        </div>
      ) : null}

      <FdSection
        title="Every run"
        badge={
          shown > 0 ? (
            <span className="fd-chip">
              {shownSandbox > 0
                ? `${plural(shown - shownSandbox, "run")} + ${plural(shownSandbox, "sandbox sim")}`
                : plural(shown, "run")}
            </span>
          ) : undefined
        }
      >
        {shown === 0 ? (
          <p className="fd-empty">No recorded bot runs yet.</p>
        ) : (
          <>
            <div className="fd-board">
              {reports.map((report) => (
                <FdBenchReportCard key={report.id} report={report} onOpen={onReportOpen} />
              ))}
            </div>
            {runsTotal + simsTotal > shown ? (
              <p className="fd-note">
                Showing the {shown} most recent of {runsTotal} runs
                {simsTotal > 0 ? ` and ${simsTotal} sandbox sims` : ""}.
              </p>
            ) : null}
          </>
        )}
      </FdSection>
    </div>
  );
}
