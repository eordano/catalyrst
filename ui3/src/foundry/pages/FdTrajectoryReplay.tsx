import { useMemo } from "react";

import { groupDigits, plural } from "../fmt";
import FdEventRow, { type FdEventVM } from "../components/FdEventRow";
import FdReplayScrubber from "../components/FdReplayScrubber";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import FdTime from "../components/FdTime";
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

/** A captured frame that still exists on the operator host, served through the
 *  evidence route — never a reconstructed image. */
export type FdReplayFrame = { name: string; url: string };

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
  /** The biography page of the scene this run played, when the header names one. */
  gameHref?: string | null;
  /** The evidence page for this run; the evidence label links there. */
  evidenceHref?: string | null;
  /** Captured frames surviving on disk; null/empty means none survive. */
  frames?: readonly FdReplayFrame[] | null;
  /** True when evidence was recorded but its directory no longer exists. */
  evidenceGone?: boolean;
  playing?: boolean;
  onPlayToggle?: () => void;
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

// --- The visual state panel, derived strictly from obs/snapshot lines. ------
// The flagtag runners print a per-seat table to stdout; when the log at or
// before the cursor holds one, the latest table renders as bars instead of
// leaving the reader to eyeball monospace. Logs without such a table render
// no panel — nothing is invented for them.

export type FdSeatReading = {
  seat: string;
  role: string;
  carried: string;
  sharePct: number;
  pickups: string;
  lost: string;
  struck: string;
};

export type FdStateReading = {
  seats: FdSeatReading[];
  flagLoose: string | null;
  tail: string | null;
  fromSeq: number;
  atSeq: number;
};

const SEAT_HEADER = /^\s*seat\s+role\s+carried\s+share/i;
const SEAT_ROW =
  /^\s*(\S+)\s+(\S+)\s+([\d.]+s)\s+([\d.]+)%\s+(\d+)\s+(\d+)\s+(\d+)\s*$/;
const FLAG_LOOSE = /^\s*flag loose\s+([\d.]+s)\s*$/;

function lineOf(data: unknown): string | null {
  if (data === null || typeof data !== "object") return null;
  const line = (data as { line?: unknown }).line;
  return typeof line === "string" ? line : null;
}

export function deriveStateReading(
  events: readonly FdReplayEventInput[],
  cursor: number,
): FdStateReading | null {
  let latest: FdStateReading | null = null;
  let current: FdStateReading | null = null;
  for (const e of events) {
    if (e.seq > cursor) break;
    if (e.type !== "obs/snapshot") continue;
    const line = lineOf(e.data);
    if (line === null) continue;
    if (SEAT_HEADER.test(line)) {
      current = { seats: [], flagLoose: null, tail: null, fromSeq: e.seq, atSeq: e.seq };
      continue;
    }
    if (!current) continue;
    const row = SEAT_ROW.exec(line);
    if (row) {
      const [, seat, role, carried, share, pickups, lost, struck] = row;
      if (seat && role && carried && share && pickups && lost && struck) {
        current.seats.push({
          seat,
          role,
          carried,
          sharePct: Number(share),
          pickups,
          lost,
          struck,
        });
        current.atSeq = e.seq;
      }
      continue;
    }
    const loose = FLAG_LOOSE.exec(line);
    if (loose) {
      current.flagLoose = loose[1] ?? null;
      current.atSeq = e.seq;
      continue;
    }
    // Any other line closes the table; a trailing totals line is kept verbatim.
    if (current.seats.length > 0) {
      current.atSeq = e.seq;
      current.tail = line.trim();
      latest = current;
    }
    current = null;
  }
  if (current && current.seats.length > 0) latest = current;
  return latest;
}

function StatePanel({ reading }: { reading: FdStateReading }) {
  const from =
    reading.fromSeq === reading.atSeq
      ? `read from the obs/snapshot line at #${reading.atSeq}`
      : `read from the obs/snapshot lines at #${reading.fromSeq}–#${reading.atSeq}`;
  return (
    <section className="fd-replay__state" aria-label="Latest recorded state">
      <p className="fd-label fd-replay__statehead" title={from}>
        Latest recorded state
        <span className="u-sr-only"> — {from}</span>
      </p>
      <ul className="fd-replay__seats">
        {reading.seats.map((s) => (
          <li className="fd-replay__seat" key={s.seat}>
            <span className="fd-replay__seatname fd-mono">{s.seat}</span>
            <span className="fd-replay__seatrole">{s.role}</span>
            <span className="fd-replay__seatbar" aria-hidden="true">
              <span
                className="fd-replay__seatfill"
                style={{ width: `${Math.min(100, Math.max(0, s.sharePct))}%` }}
              />
            </span>
            <span className="fd-replay__seatfacts">
              carried {s.carried} · {s.sharePct}% · {s.pickups} pickups ·{" "}
              {s.lost} lost · {s.struck} struck
            </span>
          </li>
        ))}
      </ul>
      {reading.flagLoose ? (
        <p className="fd-replay__stateline">flag loose {reading.flagLoose}</p>
      ) : null}
      {reading.tail ? (
        <p className="fd-replay__stateline">{reading.tail}</p>
      ) : null}
    </section>
  );
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
  gameHref = null,
  evidenceHref = null,
  frames = null,
  evidenceGone = false,
  playing = false,
  onPlayToggle,
}: FdTrajectoryReplayProps) {
  const { rows, summary } = useMemo(() => decorate(events, cursor), [events, cursor]);
  const reading = useMemo(() => deriveStateReading(events, cursor), [events, cursor]);
  const last = events[events.length - 1];
  const maxSeq = last ? last.seq : 0;
  const total = eventCount ?? events.length;
  const finish = header.finishReason;
  const checksTotal = summary.passed + summary.failed;

  return (
    <div className="fd-stack fd-replay">
      <FdPageHead
        eyebrow="Run logs"
        title={header.sceneTitle ?? "Run log"}
        crumbs={<a href={backHref}>← All runs</a>}
      />

      <dl className="fd-facts fd-replay__facts">
        <div>
          <dt>Run</dt>
          <dd className="fd-mono">{header.id}</dd>
        </div>
        {header.sceneId ? (
          <div>
            <dt>Scene</dt>
            <dd className="fd-mono">
              {gameHref ? <a href={gameHref}>{header.sceneId}</a> : header.sceneId}
            </dd>
          </div>
        ) : null}
        <div>
          <dt>Runner</dt>
          <dd>
            {header.runner === "arena" ? (
              // The arena-row rule: the raw runner value never surfaces as
              // prose — the chip speaks, the title holds the stored value.
              <span className="fd-chip" title={`runner: ${header.runner}`}>
                sandbox
                <span className="u-sr-only"> (stored value: arena)</span>
              </span>
            ) : header.runner === "dclbots" ? (
              <span className="fd-chip" title={`runner: ${header.runner}`}>
                playtest harness
                <span className="u-sr-only"> (stored value: dclbots)</span>
              </span>
            ) : header.runner ? (
              <span className="fd-chip">{header.runner}</span>
            ) : (
              <span className="fd-chip" title={`provenance: ${header.provenance}`}>
                {header.provenance}
              </span>
            )}
          </dd>
        </div>
        <div>
          <dt>Recorded</dt>
          <dd>
            <FdTime iso={header.createdAt} title={header.createdAt}>
              {stamp(header.createdAt)}
            </FdTime>
          </dd>
        </div>
        <div>
          <dt>Events</dt>
          <dd>{groupDigits(total)}</dd>
        </div>
        <div>
          <dt>Finish reason</dt>
          <dd>
            {finish?.kind ?? "not recorded"}
            {finish?.detail ? ` (${finish.detail})` : ""}
          </dd>
        </div>
        {header.parentTrajectoryId ? (
          <div>
            <dt>Forked from</dt>
            <dd className="fd-mono">
              <a
                href={replayHref(header.parentTrajectoryId)}
                title={header.parentTrajectoryId}
              >
                {shortId(header.parentTrajectoryId)}
              </a>
              {header.seedLength === null
                ? ""
                : ` · ${plural(header.seedLength, "seeded event")}`}
            </dd>
          </div>
        ) : null}
        {header.evidencePath ? (
          <div>
            <dt>Evidence</dt>
            <dd className="fd-mono">
              {evidenceHref ? (
                <a href={evidenceHref}>{header.evidencePath}</a>
              ) : (
                header.evidencePath
              )}
            </dd>
          </div>
        ) : null}
      </dl>

      {oversize ? (
        <p className="fd-empty">
          {`This log holds ${groupDigits(total)} events${
            limit ? `, past the ${groupDigits(limit)}-event inline limit` : ""
          }.`}
        </p>
      ) : (
        <>
          <div className="fd-statrow">
            <FdStat label="Turns" value={summary.turns} title="turn/start events in the log" />
            <FdStat label="Steps" value={summary.steps} title="step/start events in the log" />
            {checksTotal > 0 ? (
              <FdStat
                label="Checks passed"
                value={`${summary.passed} of ${checksTotal}`}
                title="check/verdict events in the log, up to the cursor"
              />
            ) : null}
          </div>

          <div className="fd-replay__transport">
            <FdReplayScrubber
              cursor={cursor}
              maxSeq={maxSeq}
              onCursor={onCursor}
              onStep={onStep}
              playing={playing}
              onPlayToggle={onPlayToggle}
            />

            {frames && frames.length > 0 ? (
              <div className="fd-replay__frames" aria-label="Captured frames">
                <p
                  className="fd-label fd-replay__frameshead"
                  title="written by the runner, in shot order — not mapped to the cursor"
                >
                  Captured frames
                </p>
                <ul className="fd-replay__framestrip">
                  {frames.map((frame) => (
                    <li key={frame.name} className="fd-replay__frame">
                      <a href={frame.url}>
                        <img src={frame.url} alt={`Captured frame ${frame.name}`} loading="lazy" />
                      </a>
                      <span className="fd-replay__framename fd-mono">{frame.name}</span>
                    </li>
                  ))}
                </ul>
              </div>
            ) : evidenceGone && header.evidencePath ? (
              <p className="fd-note">
                No captured frames survive for this run — the evidence recorded
                at {header.evidencePath} is no longer present on the operator
                host.
              </p>
            ) : null}
          </div>

          {reading ? <StatePanel reading={reading} /> : null}

          <FdSection
            title="Events"
            badge={
              <span className="fd-chip" title="events through the cursor">
                {groupDigits(rows.length)}
              </span>
            }
          >
            {rows.length === 0 ? (
              <p className="fd-empty">This run recorded a header but no events.</p>
            ) : (
              <ol className="fd-replay__ledger">
                {rows.map((row) => (
                  <FdEventRow
                    key={row.seq}
                    event={row}
                    onSelect={onCursor}
                    selected={row.seq === cursor}
                  />
                ))}
              </ol>
            )}
          </FdSection>
        </>
      )}

    </div>
  );
}
