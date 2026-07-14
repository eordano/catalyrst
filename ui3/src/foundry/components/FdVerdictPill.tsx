export { failedChecksPhrase } from "../checks";
import "./fdverdictpill.css";

/** The bench speaks in pass/fail — a check that cannot be evaluated fails. The
 *  older three-way program verdicts stay in the union so nothing that still
 *  renders one has to be touched to add the fourth. */
export type FdVerdictValue = "pass" | "fail" | "watch" | "kill";

type FdVerdictPillProps = {
  verdict: FdVerdictValue;
  /** Longer wording for the pill text (e.g. "playtest failed"); colour still keys
   *  off the verdict itself. */
  label?: string;
  className?: string;
};

/** How a run's stored verdict reads to a visitor. The harness counts a check
 *  it cannot evaluate as failed (the stored verdict stays honest), but a card
 *  that says "failed" when every failure is a harness gap blames the game for
 *  the harness — so when genuine failures are zero the pill turns neutral and
 *  counts what WAS verified. One function so the shelf card and the game
 *  header cannot drift apart. */

export function runVerdictReading(run: {
  verdict: "pass" | "fail";
  checksFailed: number | null;
  checksTotal: number | null;
  checksUnevaluable: number | null;
}): { verdict: "pass" | "fail" | "watch"; label: string; detail: string | null } {
  const failed = run.checksFailed;
  const total = run.checksTotal;
  const unevaluable = run.checksUnevaluable ?? 0;
  if (run.verdict === "pass") {
    return {
      verdict: "pass",
      label: "passed",
      detail:
        failed !== null && total !== null
          ? `${total - failed} of ${total} checks passed`
          : null,
    };
  }
  if (failed === null || total === null) {
    return { verdict: "fail", label: "failed", detail: null };
  }
  const genuine = Math.max(0, failed - unevaluable);
  if (genuine === 0 && unevaluable > 0) {
    return {
      verdict: "watch",
      label: `${total - failed} of ${total} verified`,
      detail: `${unevaluable} of ${total} checks could not be evaluated (harness gaps) — no genuine failures`,
    };
  }
  return {
    verdict: "fail",
    label: "failed",
    detail:
      `${genuine} of ${total} checks failed` +
      (unevaluable > 0 ? `, ${unevaluable} more not evaluable` : ""),
  };
}

/** The only place verdict colour exists. Charts, funnels and bars stay neutral. */
export default function FdVerdictPill({
  verdict,
  label,
  className = "",
}: FdVerdictPillProps) {
  return (
    <span className={"fd-verdict fd-verdict--" + verdict + (className ? " " + className : "")}>
      <span className="fd-verdict__dot" aria-hidden="true" />
      {label ?? verdict}
    </span>
  );
}
