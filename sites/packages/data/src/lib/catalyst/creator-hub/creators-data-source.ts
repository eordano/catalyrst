import { z } from "zod";

/*
 * The creators-data metrics artifact.
 *
 * `decentraland.org/creators-data` currently serves the marketing SPA: every
 * path under it answers 200 with HTML, so a JSON read fails and the ledger says
 * so. If it is ever deployed for real, the payload still decides whether any
 * number may be rendered -- the artifact carries its own `source`, and only
 * `"metabase"` is a real export. The one on disk today says `"fixture"`.
 *
 * This is spec S5.5 rule 5 in data form: a synthetic snapshot behind a warning
 * chip is still a lie, so a non-metabase artifact never becomes a `snapshot`.
 */

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

/**
 * Decides whether an artifact payload may be rendered at all. Pure, so the rule
 * is testable without a network: `creators-data-source.test.ts` asserts that
 * `source: "fixture"` maps to unavailable and only `"metabase"` maps to snapshot.
 */
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
