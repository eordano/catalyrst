import "./fdverdictpill.css";

/** The bench speaks in pass/fail — a check that cannot be evaluated fails. The
 *  older three-way program verdicts stay in the union so nothing that still
 *  renders one has to be touched to add the fourth. */
export type FdVerdictValue = "pass" | "fail" | "watch" | "kill";

type FdVerdictPillProps = {
  verdict: FdVerdictValue;
  className?: string;
};

/** The only place verdict colour exists. Charts, funnels and bars stay neutral. */
export default function FdVerdictPill({ verdict, className = "" }: FdVerdictPillProps) {
  return (
    <span className={"fd-verdict fd-verdict--" + verdict + (className ? " " + className : "")}>
      <span className="fd-verdict__dot" aria-hidden="true" />
      {verdict}
    </span>
  );
}
