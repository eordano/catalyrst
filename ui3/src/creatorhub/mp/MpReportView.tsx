import { LANE_CLAIMS, type MpLane } from "./rules";
import type { MpReportData, MpTimelineEntry, MpVerdict } from "./types";

import "./mpreportview.css";

export type MpPerfSeries = { client: string; fps: number[] };

export type MpReportViewProps = {
  runId: string;
  lane: MpLane;
  report: MpReportData | null;
  verdict: MpVerdict | null;
  timeline: MpTimelineEntry[];
  perf: MpPerfSeries[];
  onOpenReportHtml?: () => void;
  onOpenReplay: () => void;
  onNewRun: () => void;
};

function Sparkline({ points }: { points: number[] }) {
  if (points.length < 2) return null;
  const w = 160;
  const h = 36;
  const max = Math.max(...points, 1);
  const min = Math.min(...points, 0);
  const span = max - min || 1;
  const path = points
    .map(
      (p, i) =>
        `${((i / (points.length - 1)) * w).toFixed(1)},${(h - ((p - min) / span) * h).toFixed(1)}`,
    )
    .join(" ");
  return (
    <svg
      className="mp-report__spark"
      viewBox={`0 0 ${w} ${h}`}
      width={w}
      height={h}
      aria-hidden="true"
    >
      <polyline points={path} fill="none" strokeWidth="1.5" />
    </svg>
  );
}

export default function MpReportView({
  runId,
  lane,
  report,
  verdict,
  timeline,
  perf,
  onOpenReportHtml,
  onOpenReplay,
  onNewRun,
}: MpReportViewProps) {
  const divergence = report?.divergence ?? [];
  return (
    <section className="mp-report" aria-label="Sync-health report">
      <header className="mp-report__head">
        <div>
          <span className="mp-report__overline">Run {runId}</span>
          <h3 className="mp-report__title">Sync-health report</h3>
        </div>
        {verdict ? (
          <span
            className={
              "mp-report__verdict " +
              (verdict.pass ? "mp-report__verdict--pass" : "mp-report__verdict--fail")
            }
            data-mp-verdict={verdict.pass ? "pass" : "fail"}
          >
            {verdict.pass ? "PASS" : "FAIL"}
          </span>
        ) : (
          <span className="mp-report__verdict" data-mp-verdict="pending">
            verdict pending
          </span>
        )}
      </header>

      <p className="mp-report__claim">{LANE_CLAIMS[lane]}</p>

      {report === null ? (
        <p className="mp-report__pending" data-mp-note="report-pending">
          Analyzer output not available yet &#x2014; the report appears once
          mp-analyzer has processed the recorded frames.
        </p>
      ) : (
        <>
          <div className="mp-report__cards">
            <div className="mp-report__card">
              <span className="mp-report__card-label">State convergence (median)</span>
              <span className="mp-report__card-value" data-mp-metric="converge-ms">
                {report.convergeMedianMs !== null
                  ? `${Math.round(report.convergeMedianMs)}`
                  : "\u{2014}"}
              </span>
              <span className="mp-report__card-unit">
                ms{report.convergeP95Ms !== null
                  ? ` \u{B7} p95 ${Math.round(report.convergeP95Ms)} ms`
                  : ""}
              </span>
            </div>
            <div className="mp-report__card">
              <span className="mp-report__card-label">Corrections</span>
              <span className="mp-report__card-value" data-mp-metric="corrections">
                {report.corrections ?? "\u{2014}"}
              </span>
              <span className="mp-report__card-unit">late-write rejections</span>
            </div>
            <div className="mp-report__card">
              <span className="mp-report__card-label">Join burst</span>
              <span className="mp-report__card-value" data-mp-metric="join-retries">
                {report.joinRetriesMax ?? "\u{2014}"}
              </span>
              <span className="mp-report__card-unit">max REQ retries</span>
            </div>
            <div className="mp-report__card">
              <span className="mp-report__card-label">Never synced</span>
              <span className="mp-report__card-value" data-mp-metric="never-synced">
                {report.neverSynced.length}
              </span>
              <span className="mp-report__card-unit">
                {report.neverSynced.length ? report.neverSynced.join(", ") : "clients"}
              </span>
            </div>
          </div>

          {Object.keys(report.perBotConvergeMs).length > 0 && (
            <section className="mp-report__panel" aria-label="Per-client convergence">
              <div className="mp-report__label">Convergence per joiner</div>
              <div className="mp-report__perbot">
                {Object.entries(report.perBotConvergeMs).map(([peer, ms]) => (
                  <span key={peer} className="mp-report__perbot-chip">
                    {peer} <strong>{Math.round(ms)} ms</strong>
                  </span>
                ))}
              </div>
            </section>
          )}

          <section className="mp-report__panel" aria-label="Divergence">
            <div className="mp-report__label">
              Divergence at 5 s checkpoints
            </div>
            <table className="mp-report__divergence" data-mp-divergence="">
              <thead>
                <tr>
                  <th>Entity</th>
                  <th>Component</th>
                  <th>Peers</th>
                  <th>Oracle fold</th>
                  <th>Beacons</th>
                  <th>Runner store</th>
                  <th>Closed</th>
                </tr>
              </thead>
              <tbody>
                {divergence.length === 0 ? (
                  <tr>
                    <td colSpan={7} className="mp-report__empty-row">
                      No divergence observed at any checkpoint.
                    </td>
                  </tr>
                ) : (
                  divergence.map((d, i) => (
                    <tr
                      key={i}
                      className={d.closed === false ? "is-open" : undefined}
                    >
                      <td>
                        <span className="mp-report__entity">#{d.entity}</span>
                      </td>
                      <td>{d.component}</td>
                      <td>{d.peers.join(", ") || "\u{2014}"}</td>
                      <td className="mp-report__hash">{d.oracle ?? "\u{2014}"}</td>
                      <td className="mp-report__hash">{d.beacon ?? "\u{2014}"}</td>
                      <td className="mp-report__hash">{d.probe ?? "\u{2014}"}</td>
                      <td>
                        {d.closed === null ? "\u{2014}" : d.closed ? "closed" : "open at end"}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
            <p className="mp-report__footnote">
              Oracle fold: LWW oracle over each peer&rsquo;s recorded frames.
              Beacons: executed-scene hashes from scene-running peers. Runner
              store: decoded REQ/RES probes &#x2014; grounds the runner&rsquo;s store
              only.
            </p>
          </section>

          {verdict && verdict.checks.length > 0 && (
            <section className="mp-report__panel" aria-label="Verdict checks">
              <div className="mp-report__label">Threshold checks</div>
              <table className="mp-report__checks" data-mp-checks="">
                <tbody>
                  {verdict.checks.map((c) => (
                    <tr key={c.name} className={c.pass ? undefined : "is-fail"}>
                      <td>{c.name}</td>
                      <td>{c.value}</td>
                      <td>vs {c.threshold}</td>
                      <td>{c.pass ? "pass" : "fail"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}

          {perf.length > 0 && (
            <section className="mp-report__panel" aria-label="Client fps">
              <div className="mp-report__label">Client fps</div>
              <div className="mp-report__perf" data-mp-perf="">
                {perf.map((p) => (
                  <div key={p.client} className="mp-report__perf-row">
                    <span className="mp-report__perf-client">{p.client}</span>
                    <Sparkline points={p.fps} />
                    <span className="mp-report__perf-last">
                      {p.fps.length ? `${Math.round(p.fps[p.fps.length - 1] ?? 0)} fps` : ""}
                    </span>
                  </div>
                ))}
              </div>
            </section>
          )}
        </>
      )}

      {timeline.length > 0 && (
        <section className="mp-report__panel" aria-label="Run timeline">
          <div className="mp-report__label">Run timeline</div>
          <ol className="mp-report__timeline">
            {timeline.map((t, i) => (
              <li key={i} data-mp-timeline-entry={t.state}>
                <span className="mp-report__time">
                  {new Date(t.at).toLocaleTimeString(undefined, { hour12: false })}
                </span>
                <span>{t.state}</span>
                {t.detail ? <span className="mp-report__detail">{t.detail}</span> : null}
              </li>
            ))}
          </ol>
        </section>
      )}

      <footer className="mp-report__actions">
        <button
          type="button"
          className="mp-report__primary"
          data-mp-action="open-replay"
          onClick={onOpenReplay}
        >
          Replay this run
        </button>
        {onOpenReportHtml && (
          <button
            type="button"
            className="mp-report__secondary"
            data-mp-action="open-report-html"
            onClick={onOpenReportHtml}
          >
            Open report.html
          </button>
        )}
        <button
          type="button"
          className="mp-report__secondary"
          data-mp-action="new-run"
          onClick={onNewRun}
        >
          New run
        </button>
      </footer>
    </section>
  );
}
