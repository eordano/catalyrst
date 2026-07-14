import type { ReactNode } from "react";
import "./fdstat.css";

type FdStatProps = {
  label: string;
  value: ReactNode;
  note?: ReactNode;
  delta?: string;
  mono?: boolean;
  /** Where the number came from, in plain words: "mirror snapshot, read Aug 15,
   *  2026". Rendered as the hover title and as an sr-only twin, since `title`
   *  is reachable by neither touch nor AT. */
  title?: string;
  className?: string;
};

export default function FdStat({
  label,
  value,
  note,
  delta,
  mono = false,
  title,
  className = "",
}: FdStatProps) {
  return (
    <div
      className={"fd-stat" + (className ? " " + className : "")}
      title={title}
    >
      <span className="fd-stat__label">{label}</span>
      <p className={"fd-stat__value" + (mono ? " fd-stat__value--mono" : "")}>
        {value}
        {delta ? <span className="fd-stat__delta">{delta}</span> : null}
      </p>
      {title ? <span className="u-sr-only"> — {title}</span> : null}
      {note ? <p className="fd-stat__note">{note}</p> : null}
    </div>
  );
}
