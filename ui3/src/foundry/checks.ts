/** The one honest phrasing for a failed check count: a check the harness
 *  could not evaluate is counted as failed but never described as a plain
 *  failure. Lives in a CSS-free module so server code and plain-tsx scripts
 *  can import it without dragging a component stylesheet along. */
export function failedChecksPhrase(counts: {
  checksFailed: number;
  checksTotal: number;
  checksUnevaluable: number;
}): string {
  const { checksFailed, checksTotal, checksUnevaluable } = counts;
  const genuine = checksFailed - checksUnevaluable;
  if (checksUnevaluable > 0 && genuine <= 0) {
    return `${checksFailed} of ${checksTotal} checks could not be evaluated — counted as failed`;
  }
  if (checksUnevaluable > 0) {
    return `${genuine} of ${checksTotal} checks failed, ${checksUnevaluable} more could not be evaluated — counted as failed`;
  }
  return `${checksFailed} of ${checksTotal} checks failed`;
}
