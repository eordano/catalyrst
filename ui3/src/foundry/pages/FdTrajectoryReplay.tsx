import { useMemo } from "react";

import EmptyState from "../../components/EmptyState";
import FdEventRow, { type FdEventVM } from "../components/FdEventRow";
import FdReplayScrubber from "../components/FdReplayScrubber";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import {
  replayHref,
  shortId,
  stamp,
  type FdFinishReason,
} from "../components/FdTrajectoryRow";
import "../components/fdstat.css";
import "./fdtrajectoryreplay.css";

export type FdReplayEventInput = {
  seq: number;
  type: string;
  time: string;
  data: unknown;
  ignorable?: boolean;
};

export type FdReplayHeaderVM = {
  id: string;
  sceneTitle?: string | null;
  sceneId?: string | null;
  provenance: "bot" | "visitor";
  runner: string | null;
  finishReason: FdFinishReason | null;
  parentTrajectoryId: string | null;
  seedLength: number | null;
  evidencePath: string | null;
  createdAt: string;
};

export type FdTrajectoryReplayProps = {
  header: FdReplayHeaderVM;
  events: readonly FdReplayEventInput[];
  /** Rows in the log; defaults to what was handed over. */
  eventCount?: number;
  oversize?: boolean;
  /** The inline replay ceiling the loader enforced; shown when it is hit. */
  limit?: number;
  cursor: number;
  onCursor: (seq: number) => void;
  onStep: (direction: "back" | "forward") => void;
  backHref: string;
};

type Summary = {
  turns: number;
  steps: number;
  passed: number;
  failed: number;
  finishKind: string | null;
  finishDetail: string | null;
};

function reasonOf(data: unknown): { kind: string; detail: string | null } | null {
  if (data === null || typeof data !== "object") return null;
  const reason = (data as { reason?: unknown }).reason;
  if (reason === null || typeof reason !== "object") return null;
  const kind = (reason as { kind?: unknown }).kind;
  const detail = (reason as { detail?: unknown }).detail;
  return typeof kind === "string"
    ? { kind, detail: typeof detail === "string" ? detail : null }
    : null;
}

function passOf(data: unknown): boolean | null {
  if (data === null || typeof data !== "object") return null;
  const value = (data as { pass?: unknown }).pass;
  return typeof value === "boolean" ? value : null;
}

function decorate(
  events: readonly FdReplayEventInput[],
  cursor: number,
): { rows: FdEventVM[]; summary: Summary } {
  const shown = events.filter((e) => e.seq <= cursor);
  const first = shown[0];
  const t0 = first ? Date.parse(first.time) : Number.NaN;
  const rows: FdEventVM[] = [];
  const turnStack: number[] = [];
  const stepStack: number[] = [];
  const summary: Summary = {
    turns: 0,
    steps: 0,
    passed: 0,
    failed: 0,
    finishKind: null,
    finishDetail: null,
  };
  let depth = 0;

  for (const e of shown) {
    const start = e.type === "turn/start" || e.type === "step/start";
    const end = e.type === "turn/end" || e.type === "step/end";
    if (end && depth > 0) depth -= 1;

    const at = Date.parse(e.time);
    const index = rows.length;
    rows.push({
      seq: e.seq,
      type: e.type,
      time: e.time,
      data: e.data,
      ignorable: e.ignorable,
      depth,
      offsetMs: Number.isNaN(at) || Number.isNaN(t0) ? null : at - t0,
      spanMs: null,
    });

    if (start) {
      (e.type === "turn/start" ? turnStack : stepStack).push(index);
      depth += 1;
      if (e.type === "turn/start") summary.turns += 1;
      else summary.steps += 1;
    }

    if (end) {
      const opened = (e.type === "turn/end" ? turnStack : stepStack).pop();
      const openedRow = opened !== undefined ? rows[opened] : undefined;
      if (openedRow) {
        const from = Date.parse(openedRow.time);
        if (!Number.isNaN(from) && !Number.isNaN(at)) openedRow.spanMs = at - from;
      }
      if (e.type === "turn/end") {
        const reason = reasonOf(e.data);
        summary.finishKind = reason?.kind ?? null;
        summary.finishDetail = reason?.detail ?? null;
      }
    }

    if (e.type === "check/verdict") {
      const verdict = passOf(e.data);
      if (verdict === true) summary.passed += 1;
      if (verdict === false) summary.failed += 1;
    }
  }

  for (const index of [...turnStack, ...stepStack]) {
    const row = rows[index];
    if (row) row.open = true;
  }
  return { rows, summary };
}

export default function FdTrajectoryReplay({
  header,
  events,
  eventCount,
  oversize,
  limit,
  cursor,
  onCursor,
  onStep,
  backHref,
}: FdTrajectoryReplayProps) {
  const { rows, summary } = useMemo(() => decorate(events, cursor), [events, cursor]);
  const last = events[events.length - 1];
  const maxSeq = last ? last.seq : 0;
  const through = `Counted from the events through seq ${cursor}.`;
  const total = eventCount ?? events.length;
  const finish = header.finishReason;

  return (
    <div className="fd-stack fd-replay">
      <FdPageHead
        eyebrow="Trajectories"
        title={`Episode ${shortId(header.id)}`}
        intro="An append-only event log. Scrub or step to move the cursor; the ledger re-derives from the log and shows nothing the log does not hold."
        aside={
          <a className="fd-replay__back" href={backHref}>
            ← All episodes
          </a>
        }
      />

      <dl className="fd-replay__facts">
        <div>
          <dt>episode</dt>
          <dd className="fd-replay__mono">{header.id}</dd>
        </div>
        <div>
          <dt>scene</dt>
          <dd>{header.sceneTitle ?? header.sceneId ?? "—"}</dd>
        </div>
        <div>
          <dt>provenance</dt>
          <dd>
            {header.provenance}
            {header.runner ? ` · ${header.runner}` : ""}
          </dd>
        </div>
        <div>
          <dt>recorded</dt>
          <dd title={header.createdAt}>{stamp(header.createdAt)}</dd>
        </div>
        <div>
          <dt>events</dt>
          <dd>{total}</dd>
        </div>
        <div>
          <dt>finish reason</dt>
          <dd>
            {finish?.kind ?? "not recorded"}
            {finish?.detail ? ` (${finish.detail})` : ""}
          </dd>
        </div>
        {header.parentTrajectoryId ? (
          <div>
            <dt>forked from</dt>
            <dd className="fd-replay__mono">
              <a
                href={replayHref(header.parentTrajectoryId)}
                title={header.parentTrajectoryId}
              >
                {shortId(header.parentTrajectoryId)}
              </a>
              {header.seedLength === null ? "" : ` · ${header.seedLength} seeded events`}
            </dd>
          </div>
        ) : null}
        {header.evidencePath ? (
          <div>
            <dt>evidence</dt>
            <dd className="fd-replay__mono">{header.evidencePath}</dd>
          </div>
        ) : null}
      </dl>

      {oversize ? (
        <EmptyState
          variant="inline"
          title="Episode too large to replay inline"
          subtitle={`This log holds ${total.toLocaleString()} events${
            limit ? `, past the ${limit.toLocaleString()}-event inline limit` : ""
          }. Nothing is summarised in its place — read it from the database or the run's evidence directory instead.`}
        />
      ) : (
        <>
          <div className="fd-statrow">
            <FdStat label="Turns" value={summary.turns} note={through} />
            <FdStat label="Steps" value={summary.steps} note={through} />
            <FdStat
              label="Checks"
              value={`${summary.passed} passed · ${summary.failed} failed`}
              note="One per check/verdict event. A check with no recorded verdict counts as neither."
            />
            <FdStat
              label="Finish reason"
              value={summary.finishKind ?? "—"}
              note={
                summary.finishKind
                  ? (summary.finishDetail ?? "The last turn/end at or before the cursor.")
                  : "No turn/end at or before the cursor."
              }
            />
          </div>

          <FdReplayScrubber
            cursor={cursor}
            maxSeq={maxSeq}
            onCursor={onCursor}
            onStep={onStep}
          />

          <FdSection title="Event ledger">
            {rows.length === 0 ? (
              <EmptyState
                variant="inline"
                title="No events recorded."
                subtitle="This episode has a header but an empty log."
              />
            ) : (
              <ol className="fd-replay__ledger">
                {rows.map((row) => (
                  <FdEventRow key={row.seq} event={row} />
                ))}
              </ol>
            )}
            <p className="fd-note">
              Spans are real: a bracket that is still open at the cursor is marked
              open and given no duration. Tool arguments and snapshots are shown as
              stored, not summarised.
            </p>
          </FdSection>
        </>
      )}

    </div>
  );
}
