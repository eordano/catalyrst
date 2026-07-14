import type { ReactNode } from "react";
import { unbuilt } from "../lib/datum";
import DatumBadge from "./DatumBadge";
import "./unbuiltpanel.css";

export type UnbuiltPanelProps = {
  title: string;
  /** Why it does not exist -- the verified reason, not an apology. */
  why: string;
  /**
   * What a creator can do instead, today. May contain a plain `<a>` or a
   * `CliEscape`. It must not contain anything that looks like the missing
   * capability: no button, no form, no toggle.
   */
  today?: ReactNode;
};

/**
 * A capability that has no backend. It renders `<section role="note">` and has
 * **no click surface**: no button, no disabled control, no form, no link that
 * could return a fake success.
 *
 * A disabled button teaches "this will work once I'm signed in". A dashed note
 * naming the missing service teaches the truth, and the truth is the point.
 */
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
