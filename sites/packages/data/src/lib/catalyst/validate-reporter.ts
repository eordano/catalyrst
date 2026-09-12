import { setValidationReporter } from "@ui/validate";

import { reportSchemaDrift } from "./schema";

export function installValidationReporter(): void {
  setValidationReporter(({ boundary, paths }) => {
    reportSchemaDrift(boundary, paths);
  });
}
