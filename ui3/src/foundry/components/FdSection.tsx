import type { ReactNode } from "react";
import { useId } from "react";
import "./fdsection.css";

type FdSectionProps = {
  title: string;
  badge?: ReactNode;
  aside?: ReactNode;
  sub?: ReactNode;
  children?: ReactNode;
  className?: string;
  id?: string;
};

export default function FdSection({
  title,
  badge,
  aside,
  sub,
  children,
  className = "",
  id,
}: FdSectionProps) {
  const headingId = useId();
  return (
    <section
      className={"fd-section" + (className ? " " + className : "")}
      aria-labelledby={headingId}
      id={id}
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

/** The one line every route renders when its records cannot be read. */
export const FD_UNAVAILABLE = "This record is not available right now.";

/** What a steward row is, said once for both surfaces that list them. */
export const FD_STEWARDS_SUB = "Recorded claims — nothing here is verified ownership.";

export function FdPageHead({
  eyebrow,
  title,
  intro,
  crumbs,
  aside,
  display = false,
}: {
  eyebrow?: string;
  title: string;
  intro?: string;
  /** Navigation line rendered directly under the title, bottom-left. */
  crumbs?: ReactNode;
  aside?: ReactNode;
  /** The front door's larger title. Opt-in, and only there. */
  display?: boolean;
}) {
  return (
    <header className={"fd-pagehead" + (display ? " fd-pagehead--display" : "")}>
      <div className="fd-pagehead__main">
        {eyebrow ? <p className="fd-pagehead__eyebrow">{eyebrow}</p> : null}
        <h1 className="fd-pagehead__title">{title}</h1>
        {intro ? <p className="fd-pagehead__intro">{intro}</p> : null}
        {crumbs ? <p className="fd-pagehead__crumbs">{crumbs}</p> : null}
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
