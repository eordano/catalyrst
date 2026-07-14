import type { ReactNode } from "react";
import "./fdstat.css";

type FdStatProps = {
  label: string;
  value: ReactNode;
  note?: string;
  delta?: string;
  mono?: boolean;
  className?: string;
};

export default function FdStat({
  label,
  value,
  note,
  delta,
  mono = false,
  className = "",
}: FdStatProps) {
  return (
    <div className={"fd-stat" + (className ? " " + className : "")}>
      <span className="fd-stat__label">{label}</span>
      <p className={"fd-stat__value" + (mono ? " fd-stat__value--mono" : "")}>
        {value}
        {delta ? <span className="fd-stat__delta">{delta}</span> : null}
      </p>
      {note ? <p className="fd-stat__note">{note}</p> : null}
    </div>
  );
}
