import { startTransition, StrictMode } from "react";
import { hydrateRoot } from "react-dom/client";
import { HydratedRouter } from "react-router/dom";

import { installValidationReporter } from "@data/lib/catalyst/validate-reporter";

// Before hydration, so a rejection during the first render is already routed.
// Without this call the seam's reporter stays null and every production
// validation failure is a console line nobody reads -- the telemetry wiring
// exists but is never armed.
installValidationReporter();

startTransition(() => {
  hydrateRoot(
    document,
    <StrictMode>
      <HydratedRouter />
    </StrictMode>,
  );
});
