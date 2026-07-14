import { LANE_STARTING_NOTE, type MpLane } from "./rules";
import type { MpBotRow, MpRunPhase, MpTimelineEntry } from "./types";

import "./mpliverunview.css";

export type MpLiveRunViewProps = {
  runId: string;
  lane: MpLane;
  phase: MpRunPhase;
  detail?: string | null;
  bots: MpBotRow[];
  timeline: MpTimelineEntry[];
  logs: string[];
  shots: string[];
  connLost?: boolean;
  onStop?: () => void;
};

const PHASE_LABELS: Record<MpRunPhase, string> = {
  queued: "Queued",
  starting: "Starting",
  running: "Running",
  analyzing: "Analyzing",
  done: "Done",
  failed: "Failed",
};

function clock(at: number): string {
  return new Date(at).toLocaleTimeString(undefined, { hour12: false });
}

export default function MpLiveRunView({
  runId,
  lane,
  phase,
  detail = null,
  bots,
  timeline,
  logs,
  shots,
  connLost = false,
  onStop,
}: MpLiveRunViewProps) {
  return (
    <section className="mp-live" data-mp-phase={phase} aria-label="Live run">
      <header className="mp-live__head">
        <div>
          <span className="mp-live__overline">Run {runId}</span>
          <h3 className="mp-live__phase">
            {PHASE_LABELS[phase]}
            {detail ? <span className="mp-live__detail"> &#xB7; {detail}</span> : null}
          </h3>
        </div>
        {onStop && phase !== "done" && phase !== "failed" && (
          <button
            type="button"
            className="mp-live__stop"
            data-mp-action="stop"
            onClick={onStop}
          >
            Stop run
          </button>
        )}
      </header>

      {connLost && (
        <p className="mp-live__warn" role="alert">
          Sidecar connection lost &#x2014; reconnecting. The run keeps going; updates
          resume when the sidecar is back.
        </p>
      )}

      {(phase === "queued" || phase === "starting") && (
        <p className="mp-live__starting" data-mp-note="starting">
          {LANE_STARTING_NOTE}
        </p>
      )}

      <div className="mp-live__columns">
        <section className="mp-live__panel" aria-label="Clients">
          <div className="mp-live__label">Clients</div>
          {bots.length === 0 ? (
            <p className="mp-live__empty">No clients reported yet.</p>
          ) : (
            <table className="mp-live__bots" data-mp-bots="">
              <thead>
                <tr>
                  <th>Peer</th>
                  <th>Connected</th>
                  <th>CRDT deltas</th>
                  <th>Server sync</th>
                  <th>Probe hash</th>
                </tr>
              </thead>
              <tbody>
                {bots.map((b) => (
                  <tr key={b.peer} data-mp-bot={b.peer}>
                    <td>{b.peer}</td>
                    <td>{b.connected ? "yes" : "no"}</td>
                    <td>{b.deltas}</td>
                    <td>{b.synced === null ? "\u{2014}" : b.synced ? "synced" : "pending"}</td>
                    <td className="mp-live__hash">{b.probe ?? "\u{2014}"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>

        <section className="mp-live__panel" aria-label="Status timeline">
          <div className="mp-live__label">Timeline</div>
          <ol className="mp-live__timeline">
            {timeline.map((t, i) => (
              <li key={i} data-mp-timeline-entry={t.state}>
                <span className="mp-live__time">{clock(t.at)}</span>
                <span className="mp-live__state">{t.state}</span>
                {t.detail ? <span className="mp-live__detail">{t.detail}</span> : null}
              </li>
            ))}
          </ol>
        </section>
      </div>

      {(lane === "engine" || lane === "mixed") && (
        <section className="mp-live__panel" aria-label="Client screenshots">
          <div className="mp-live__label">Screenshots</div>
          {shots.length === 0 ? (
            <p className="mp-live__empty">
              No shots yet &#x2014; engine clients capture at checkpoints.
            </p>
          ) : (
            <div className="mp-live__shots" data-mp-shots="">
              {shots.map((src, i) => (
                <img key={i} src={src} alt={`Client shot ${i + 1}`} />
              ))}
            </div>
          )}
        </section>
      )}

      <section className="mp-live__panel" aria-label="Log tail">
        <div className="mp-live__label">Log tail</div>
        <pre className="mp-live__logs" data-mp-logs="">
          {logs.length === 0 ? "\u{2026}" : logs.join("\n")}
        </pre>
      </section>
    </section>
  );
}
