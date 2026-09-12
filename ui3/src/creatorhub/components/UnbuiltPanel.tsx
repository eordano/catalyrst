import type { ReactNode } from "react";
import { unbuilt } from "../lib/datum";
import DatumBadge from "./DatumBadge";
import "./unbuiltpanel.css";

export type UnbuiltPanelProps = {
  title: string;
  why: string;
  today?: ReactNode;
};

export default function UnbuiltPanel({ title, why, today }: UnbuiltPanelProps) {
  return (
    <section className="ub" role="note">
      <div className="ub__head">
        <h3 className="ub__title">{title}</h3>
        <DatumBadge datum={unbuilt(title, why, null)} />
      </div>
      <p className="ub__why">{why}</p>
      {today != null ? (
        <div className="ub__today">
          <span className="ub__todaylabel">Today</span>
          <div className="ub__todaybody">{today}</div>
        </div>
      ) : null}
    </section>
  );
}
