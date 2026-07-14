import type { ReactNode } from "react";

type FdTimeProps = {
  /** Machine-readable instant backing the visible text. */
  iso: string;
  /** Visible rendering of the moment. */
  children: ReactNode;
  /** Stamp appended for screen readers only — e.g. the absolute time behind a
   *  relative offset, or the full ISO behind a day-only label. */
  sr?: string;
  className?: string;
  /** Hover stamp; pass the ISO to keep the pointer affordance. */
  title?: string;
};

/** A rendered timestamp. The instant always travels in dateTime, so it is
 *  reachable without hover — the title-only-ISO pattern retired. */
export default function FdTime({ iso, children, sr, className, title }: FdTimeProps) {
  return (
    <time dateTime={iso} className={className} title={title}>
      {children}
      {sr ? <span className="u-sr-only"> ({sr})</span> : null}
    </time>
  );
}
