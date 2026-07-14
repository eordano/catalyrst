import type { ReactNode } from "react";
import { useId } from "react";
import "./fdsection.css";

type FdSectionProps = {
  title: string;
  badge?: ReactNode;
  aside?: ReactNode;
  sub?: string;
  children?: ReactNode;
  className?: string;
};

export default function FdSection({
  title,
  badge,
  aside,
  sub,
  children,
  className = "",
}: FdSectionProps) {
  const headingId = useId();
  return (
    <section
      className={"fd-section" + (className ? " " + className : "")}
      aria-labelledby={headingId}
    >
      <div className="fd-section__head">
        <h2 className="fd-section__title" id={headingId}>
          {title}
          {badge}
        </h2>
        {aside ? <div className="fd-section__aside">{aside}</div> : null}
      </div>
      {sub ? <p className="fd-section__sub">{sub}</p> : null}
      {children}
    </section>
  );
}

/** The provenance the standing note renders instead of asserting. Every field
 *  is a row count or a value read from a deployment entity. */
export type FdProvenance = {
  worldGames: number;
  deployFrom: string | null;
  deployTo: string | null;
  botRuns: number;
  trajectories: number;
  gddDocs: number;
  tokens: number;
};

const MONTHS = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

function monthYear(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : `${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

function deployRange(from: string | null, to: string | null): string {
  if (!from || !to) return "";
  const a = monthYear(from);
  const b = monthYear(to);
  return a === b ? ` (${a})` : ` (${a}–${b})`;
}

/** A single provenance fact: the real count, or an honest "none yet". */
function Fact({ n, one, many, none }: { n: number; one: string; many: string; none: string }) {
  const label = n === 0 ? none : `${n.toLocaleString()} ${n === 1 ? one : many}`;
  return (
    <li className={"fd-datanote__fact" + (n === 0 ? " fd-datanote__fact--empty" : "")}>
      {label}
    </li>
  );
}

/**
 * The standing note. It closes the front door and every console page. It does
 * not claim the data is real — it renders the data, so the claim is the count.
 * With no database behind it, it says that, rather than asserting anything.
 */
export function FdDataNote({ provenance }: { provenance?: FdProvenance | null }) {
  if (!provenance) {
    return (
      <p className="fd-datanote" role="note">
        This site reads every figure from a database. None is configured here, so
        it shows nothing rather than inventing it.
      </p>
    );
  }
  const p = provenance;
  return (
    <div className="fd-datanote" role="note">
      <p className="fd-datanote__lead">What is real here, right now:</p>
      <ul className="fd-datanote__facts">
        <Fact
          n={p.worldGames}
          one={`game deployed to Decentraland Worlds${deployRange(p.deployFrom, p.deployTo)} — dated from its deployment entity`}
          many={`games deployed to Decentraland Worlds${deployRange(p.deployFrom, p.deployTo)} — dated from their deployment entities`}
          none="No games imported from Worlds yet"
        />
        <Fact n={p.botRuns} one="bot run recorded" many="bot runs recorded" none="No bot runs recorded yet" />
        <Fact n={p.trajectories} one="trajectory logged" many="trajectories logged" none="No trajectories logged yet" />
        <Fact n={p.gddDocs} one="design doc imported" many="design docs imported" none="No design docs imported yet" />
        <Fact n={p.tokens} one="copilot token metered" many="copilot tokens metered" none="No copilot tokens metered yet" />
      </ul>
      <p className="fd-datanote__foot">
        Each figure is a row count or a value read from a deployment entity —
        recorded from real executions, not typed in. Where a count is zero, that
        section shows an empty state. Nothing here is invented.
      </p>
    </div>
  );
}

export function FdPageHead({
  eyebrow,
  title,
  intro,
  aside,
}: {
  eyebrow?: string;
  title: string;
  intro?: string;
  aside?: ReactNode;
}) {
  return (
    <header className="fd-pagehead">
      <div className="fd-pagehead__main">
        {eyebrow ? <p className="fd-pagehead__eyebrow">{eyebrow}</p> : null}
        <h1 className="fd-pagehead__title">{title}</h1>
        {intro ? <p className="fd-pagehead__intro">{intro}</p> : null}
      </div>
      {aside ? <div className="fd-pagehead__aside">{aside}</div> : null}
    </header>
  );
}

/** In-app navigation placeholder. First paint is always served with data. */
export function FdSkeletonRows({ rows = 5 }: { rows?: number }) {
  return (
    <div className="u-skel fd-skel" aria-hidden="true">
      {Array.from({ length: rows }, (_, i) => (
        <span key={i} className="u-skel__line fd-skel__line" />
      ))}
    </div>
  );
}
