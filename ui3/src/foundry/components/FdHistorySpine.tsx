import { dayUTC } from "../fmt";
import FdTime from "./FdTime";
import FdVerdictPill from "./FdVerdictPill";
import "./fdcellchip.css";
import "./fdhistoryspine.css";

/** Three dot treatments, and nothing else: designed (blue), the game in the
 *  world (amber), played (green). A hollow dot means nothing is recorded yet. */
export type FdHistoryNodeKind = "designed" | "built" | "live" | "played";

/** One recorded fact on the game's spine. Every node carries exactly one
 *  anchor; a node with no `href` renders its label as plain text. */
export type FdHistoryNodeVM = {
  key: string;
  kind: FdHistoryNodeKind;
  /** ISO instant of the recorded fact; null renders no date column. */
  at: string | null;
  href?: string | null;
  label: string;
  /** Muted text before the anchor (the no-design-doc line). */
  prefix?: string;
  title?: string;
  chip?: { text: string; title?: string } | null;
  verdict?: "pass" | "fail" | "watch" | null;
  /** The shared verdict reading's label + detail — the pill never renders a
   *  bare tier name when the caller has the words. */
  verdictLabel?: string | null;
  verdictTitle?: string | null;
  muted?: boolean;
};

/** A derived upcoming occurrence — after the TODAY rule, hollow dot. */
export type FdHistoryFutureVM = {
  key: string;
  at: string;
  label: string;
  href: string;
};

/** This program's dated reading of the game. It is a judgment about the game,
 *  not something the game did, so it sits beside the spine and never on it. */
export type FdHistoryReadingVM = {
  key: string;
  href: string;
  label: string;
  title: string;
};

export type FdHistorySpineProps = {
  /** Oldest first. */
  nodes: readonly FdHistoryNodeVM[];
  todayIso: string;
  futures?: readonly FdHistoryFutureVM[];
  readings?: readonly FdHistoryReadingVM[];
  allRunsHref?: string | null;
  onNodeOpen?: (kind: FdHistoryNodeKind | "upcoming" | "reading") => void;
};

export default function FdHistorySpine({
  nodes,
  todayIso,
  futures = [],
  readings = [],
  allRunsHref = null,
  onNodeOpen,
}: FdHistorySpineProps) {
  return (
    <div className="fd-life-wrap">
      <ol className="fd-life">
        {nodes.map((n) => (
          <li
            key={n.key}
            className={
              `fd-life__node fd-life__node--${n.kind}` +
              (n.muted ? " fd-life__node--muted" : "")
            }
          >
            <span className="fd-life__dot" aria-hidden="true" />
            {n.at ? (
              <FdTime iso={n.at} className="fd-life__date fd-mono">
                {dayUTC(n.at)}
              </FdTime>
            ) : (
              <span className="fd-life__date" aria-hidden="true" />
            )}
            <span className="fd-life__body">
              {n.prefix ? <span className="fd-life__prefix">{n.prefix}</span> : null}
              {n.href ? (
                <a href={n.href} title={n.title} onClick={() => onNodeOpen?.(n.kind)}>
                  {n.label}
                </a>
              ) : (
                <span title={n.title}>{n.label}</span>
              )}
              {n.chip ? (
                <span className="fd-chip" title={n.chip.title}>
                  {n.chip.text}
                </span>
              ) : null}
              {n.verdict ? (
                <span title={n.verdictTitle ?? undefined}>
                  <FdVerdictPill
                    verdict={n.verdict}
                    label={n.verdictLabel ?? undefined}
                  />
                </span>
              ) : null}
            </span>
          </li>
        ))}
        <li className="fd-life__today">
          <span className="fd-life__todayrule" aria-hidden="true" />
          <span className="fd-life__todaydate fd-mono">today · {dayUTC(todayIso)}</span>
        </li>
        {futures.map((f) => (
          <li key={f.key} className="fd-life__node fd-life__node--upcoming">
            <span className="fd-life__dot" aria-hidden="true" />
            <FdTime iso={f.at} className="fd-life__date fd-mono">
              {dayUTC(f.at)}
            </FdTime>
            <span className="fd-life__body">
              <a href={f.href} onClick={() => onNodeOpen?.("upcoming")}>
                {f.label}
              </a>
            </span>
          </li>
        ))}
      </ol>
      {readings.length > 0 ? (
        <div className="fd-life__readings">
          <p className="fd-label">Read as the same concept</p>
          <div className="fd-chiprow">
            {readings.map((r) => (
              <a
                key={r.key}
                className="fd-cellchip fd-cellchip--link"
                href={r.href}
                title={r.title}
                onClick={() => onNodeOpen?.("reading")}
              >
                {r.label}
                <span className="u-sr-only"> — {r.title}</span>
              </a>
            ))}
          </div>
        </div>
      ) : null}
      {allRunsHref ? (
        <p className="fd-life__foot">
          <a href={allRunsHref}>All runs</a>
        </p>
      ) : null}
    </div>
  );
}
