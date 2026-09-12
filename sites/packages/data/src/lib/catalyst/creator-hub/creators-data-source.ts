import { z } from "zod";

export const METRICS_PATH_PREFIX = "/worlds";
export const METRICS_PATH_SUFFIX = "/metrics";

export function worldMetricsPath(world: string): string {
  return `${METRICS_PATH_PREFIX}/${encodeURIComponent(world)}${METRICS_PATH_SUFFIX}`;
}

const strOrNull = z
  .string()
  .nullish()
  .transform((v) => v ?? null);

export const MetricsArtifactSchema = z.object({
  source: strOrNull,
  exportedAt: strOrNull,
  exported_at: strOrNull,
  generatedAt: strOrNull,
  metrics: z.unknown().nullish(),
  data: z.unknown().nullish(),
});
export type MetricsArtifact = z.infer<typeof MetricsArtifactSchema>;

export type ArtifactVerdict =
  | { kind: "snapshot"; exportSource: "metabase"; exportedAt: string; value: unknown }
  | { kind: "unavailable"; reason: string };

const UNSTATED = "an unstated date";

export function classifyMetricsArtifact(raw: unknown): ArtifactVerdict {
  const parsed = MetricsArtifactSchema.safeParse(raw);
  if (!parsed.success) {
    return {
      kind: "unavailable",
      reason:
        "The response did not carry a metrics artifact (the host serves the marketing SPA, so a JSON read gets HTML). No values are shown.",
    };
  }
  const a = parsed.data;
  const exportedAt = a.exportedAt ?? a.exported_at ?? a.generatedAt ?? UNSTATED;

  if (a.source === null) {
    return {
      kind: "unavailable",
      reason:
        "The metrics artifact does not say where it was exported from. An export with no stated source is not shown.",
    };
  }
  if (a.source !== "metabase") {
    return {
      kind: "unavailable",
      reason: `The metrics artifact currently loaded is source: ${a.source} (synthetic), exported ${exportedAt}. No values are shown.`,
    };
  }
  return {
    kind: "snapshot",
    exportSource: "metabase",
    exportedAt,
    value: a.metrics ?? a.data ?? null,
  };
}
