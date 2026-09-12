import contract from "./telemetry-contract.json";
import { validateEventAgainst, type TelemetryContract } from "./validate";

export function warnIfInvalid(event: string, props: Record<string, unknown>): void {
  try {
    const problems = validateEventAgainst(contract as unknown as TelemetryContract, event, props);
    if (problems.length > 0) {
      console.error(
        `[telemetry] event "${event}" violates its contract: ${problems.join("; ")}`,
      );
    }
  } catch {
  }
}
