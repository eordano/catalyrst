import { z } from "zod";

import {
  type ControlResult,
  available,
  unavailable,
  unavailableFromStatus,
} from "./availability";

const ExperimentSchema = z.object({
  exp_key: z.string(),
  exposures: z.coerce.number(),
  variants: z.array(z.string()),
  metrics: z.array(z.object({ event: z.string(), count: z.coerce.number() })),
  control: z.string().optional(),
  reason: z.string().optional(),
});

const DashSchema = z.object({
  experiments: z.array(ExperimentSchema),
  unreadable: z.array(ExperimentSchema),
});

export type ExperimentRow = z.infer<typeof ExperimentSchema>;

export type ExperimentsStatus = {
  readable: ExperimentRow[];
  unreadable: ExperimentRow[];
};

function telemetryBase(): string | undefined {
  const b = process.env.TELEMETRY_URL;
  return b && b.trim() !== "" ? b.replace(/\/$/, "") : undefined;
}

const SERVER_CHECK =
  "catalyrst-telemetry/src/handlers/experiments.rs:40 (GET /dash/experiments, public)";

export async function loadExperimentsStatus(
  signal?: AbortSignal,
): Promise<ControlResult<ExperimentsStatus>> {
  const base = telemetryBase();
  if (!base) {
    return unavailable("not-configured", "TELEMETRY_URL is not set.", {
      serverCheck: SERVER_CHECK,
    });
  }

  let res: Response;
  try {
    res = await fetch(`${base}/dash/experiments`, {
      signal,
      headers: { accept: "application/json" },
    });
  } catch {
    return unavailable(
      "backend-error",
      "Telemetry service did not answer the experiments dashboard.",
      { serverCheck: SERVER_CHECK },
    );
  }

  if (!res.ok) {
    return unavailableFromStatus(
      res.status,
      `Experiments dashboard returned ${res.status}.`,
      SERVER_CHECK,
    );
  }

  const parsed = DashSchema.safeParse(await res.json().catch(() => null));
  if (!parsed.success) {
    return unavailable(
      "backend-error",
      "Experiments dashboard returned an unexpected shape.",
      { serverCheck: SERVER_CHECK },
    );
  }

  return available({
    readable: parsed.data.experiments,
    unreadable: parsed.data.unreadable,
  });
}
