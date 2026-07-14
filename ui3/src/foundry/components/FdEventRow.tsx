import type { CSSProperties } from "react";

import { groupDigits, stampUTC } from "../fmt";
import { basename } from "./evidence";
import FdTime from "./FdTime";
import "./fdeventrow.css";

export type FdEventVM = {
  seq: number;
  type: string;
  time: string;
  data: unknown;
  ignorable?: boolean;
  /** Bracket nesting, computed by the replay page from turn/step events. */
  depth: number;
  /** Milliseconds since the first event; null when a time could not be read. */
  offsetMs: number | null;
  /** Only set for a bracket whose close is inside the shown range. */
  spanMs: number | null;
  /** A bracket still open at the cursor: it gets no duration, ever. */
  open?: boolean;
};

const MAX_JSON_CHARS = 4000;

function record(data: unknown): Record<string, unknown> | null {
  return data !== null && typeof data === "object" && !Array.isArray(data)
    ? (data as Record<string, unknown>)
    : null;
}

function str(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function pretty(value: unknown): { text: string; storedChars: number | null } {
  const text = JSON.stringify(value, null, 2) ?? String(value);
  return text.length > MAX_JSON_CHARS
    ? { text: text.slice(0, MAX_JSON_CHARS), storedChars: text.length }
    : { text, storedChars: null };
}

export function formatMs(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  return `${m}m ${Math.round((ms % 60_000) / 1000)}s`;
}

function JsonBlob({ value }: { value: unknown }) {
  const { text, storedChars } = pretty(value);
  return (
    <>
      <pre className="fd-ev__pre">{text}</pre>
      {storedChars === null ? null : (
        <p className="fd-ev__trunc">
          Truncated for display — {groupDigits(storedChars)} characters stored.
        </p>
      )}
    </>
  );
}

function Body({ type, data }: { type: string; data: unknown }) {
  const d = record(data);

  if (type === "turn/start" || type === "turn/end") {
    const reason = record(d?.reason);
    return (
      <span className="fd-ev__line">
        turn {String(d?.turn ?? "?")}
        {reason ? (
          <>
            {" — "}
            <span className="fd-ev__reason">{String(reason.kind)}</span>
            {str(reason.detail) ? ` (${str(reason.detail)})` : null}
          </>
        ) : null}
      </span>
    );
  }

  if (type === "step/start" || type === "step/end") {
    return (
      <span className="fd-ev__line">
        turn {String(d?.turn ?? "?")} · step {String(d?.step ?? "?")}
      </span>
    );
  }

  if (type === "tool/call") {
    const args = d?.arguments;
    return (
      <div className="fd-ev__body">
        <span className="fd-ev__name">{str(d?.name) ?? "unnamed tool"}</span>
        {str(d?.callId) ? <span className="fd-ev__callid">{str(d?.callId)}</span> : null}
        {typeof args === "string" ? (
          <pre className="fd-ev__pre">{args}</pre>
        ) : args === undefined ? null : (
          <JsonBlob value={args} />
        )}
      </div>
    );
  }

  if (type === "check/verdict") {
    const passed = d?.pass === true;
    return (
      <div className="fd-ev__body">
        <span className={"fd-ev__verdict" + (passed ? " is-pass" : " is-fail")}>
          {passed ? "PASS" : "FAIL"}
        </span>
        <span className="fd-ev__name">{str(d?.kind) ?? "check"}</span>
        {str(d?.detail) ? <span className="fd-ev__detail">{str(d?.detail)}</span> : null}
        {str(d?.why) ? <p className="fd-ev__why">{str(d?.why)}</p> : null}
      </div>
    );
  }

  if (type === "obs/snapshot") {
    const line = str(d?.line);
    const shots = Array.isArray(d?.shots)
      ? (d.shots as unknown[]).filter((s): s is string => typeof s === "string")
      : [];
    // The raw payload can carry absolute host paths — the shot files live in
    // operator-side evidence dirs. Basename the shots before the JsonBlob fallback
    // prints the payload whole, the same way the explicit shot list below does.
    const safeData =
      d && shots.length > 0 ? { ...d, shots: shots.map(basename) } : data;
    return (
      <div className="fd-ev__body">
        {str(d?.stream) ? (
          <span className="fd-ev__callid">{str(d?.stream)}</span>
        ) : null}
        {line === null ? <JsonBlob value={safeData} /> : (
          <pre className="fd-ev__pre fd-ev__pre--verbatim">{line}</pre>
        )}
        {shots.length > 0 ? (
          <div className="fd-ev__shots">
            <span
              className="fd-ev__shotslabel"
              title="file names only — evidence directories are not served from here"
            >
              screenshots written by the runner
            </span>
            <ul>
              {shots.map((shot) => (
                <li key={shot}>{basename(shot)}</li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
    );
  }

  if (type === "run/end-seed") {
    return (
      <span className="fd-ev__line">
        Seed boundary — everything above was inherited from the parent episode.
      </span>
    );
  }

  // An unrecognised event type has no dedicated view. Rather than pretty-print
  // the whole payload — which could publish an internal path a future runner put
  // there — the row states only that a payload exists and how large it is.
  let storedChars = 0;
  try {
    storedChars = JSON.stringify(data)?.length ?? 0;
  } catch {
    storedChars = 0;
  }
  return (
    <div className="fd-ev__body">
      <p className="fd-ev__omitted">
        Payload not rendered — {groupDigits(storedChars)} characters stored.
      </p>
    </div>
  );
}

export type FdEventRowProps = {
  event: FdEventVM;
  /** When set, the whole row is a button that moves the replay cursor here. */
  onSelect?: (seq: number) => void;
  /** The row the cursor currently sits on. */
  selected?: boolean;
};

/** One row of the event ledger. Every field shown is a field the log holds. */
export default function FdEventRow({ event, onSelect, selected = false }: FdEventRowProps) {
  const bracket =
    event.type.startsWith("turn/") || event.type.startsWith("step/");
  const style: CSSProperties & { "--fd-ev-depth": number } = {
    "--fd-ev-depth": event.depth,
  };
  return (
    <li
      className={
        "fd-ev" +
        (bracket ? " fd-ev--bracket" : "") +
        (event.open ? " is-open" : "") +
        (onSelect ? " fd-ev--seek" : "") +
        (selected ? " is-selected" : "")
      }
      style={style}
    >
      {onSelect ? (
        // Stretched over the row: one click anywhere on it moves the cursor.
        <button
          type="button"
          className="fd-ev__seek"
          onClick={() => onSelect(event.seq)}
          aria-label={`Move the replay cursor to event ${event.seq}`}
          aria-pressed={selected}
        />
      ) : null}
      <span className="fd-ev__seq">#{event.seq}</span>
      {Number.isNaN(Date.parse(event.time)) ? (
        <span className="fd-ev__at" title={event.time}>
          {event.offsetMs === null ? "—" : `+${formatMs(event.offsetMs)}`}
          <span className="u-sr-only"> ({event.time})</span>
        </span>
      ) : (
        <FdTime
          iso={event.time}
          className="fd-ev__at"
          title={event.time}
          sr={stampUTC(event.time)}
        >
          {event.offsetMs === null ? "—" : `+${formatMs(event.offsetMs)}`}
        </FdTime>
      )}
      <span className="fd-ev__type">{event.type}</span>
      <div className="fd-ev__main">
        <Body type={event.type} data={event.data} />
        {event.spanMs !== null ? (
          <span className="fd-ev__span">{formatMs(event.spanMs)}</span>
        ) : null}
        {event.open ? <span className="fd-ev__span">still open here</span> : null}
        {event.ignorable ? <span className="fd-ev__chip">ignorable</span> : null}
      </div>
    </li>
  );
}
