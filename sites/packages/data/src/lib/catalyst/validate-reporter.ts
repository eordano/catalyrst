import { setValidationReporter } from "@ui/validate";

import { reportSchemaDrift } from "./schema";

/**
 * Point ui3's `check()` at the drift reporting this repo already has.
 *
 * `check` lives in ui3 and `track` lives in sites, and the dependency runs
 * sites -> ui3 only -- so ui3 cannot import the transport and sites installs it
 * instead. Unset, a production rejection is a console line nobody reads.
 *
 * It routes through `reportSchemaDrift` rather than calling `track` directly so
 * every schema rejection in the app, whether from a hand-written guard in this
 * package or from a generated schema at a ui3 boundary, arrives as one event
 * with one shape. A second spelling of the same event would split the only
 * query that answers "is something drifting".
 *
 * Paths, never values: a rejected payload is exactly the kind of thing likely
 * to hold a wallet address or a chat line, and the shape alone is what makes
 * drift diagnosable.
 */
export function installValidationReporter(): void {
  setValidationReporter(({ boundary, paths }) => {
    reportSchemaDrift(boundary, paths);
  });
}
