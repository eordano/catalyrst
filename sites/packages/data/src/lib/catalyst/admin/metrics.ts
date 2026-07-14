import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import type { Envelope } from "../schema";
import { unavailable, type Unavailable } from "./availability";

import fixtureJson from "../../../fixtures/admin-metrics.json";

export const SURFACES = ["places", "communities", "events"] as const;
export type SurfaceKey = (typeof SURFACES)[number];

export const RANGES = ["7d", "30d"] as const;
export type Range = (typeof RANGES)[number];

const QueueSchema = z.record(z.string(), z.number());

const SurfaceSchema = z.object({
  key: z.enum(["places", "communities", "events"]),
  label: z.string(),
  service: z.string(),
  listEndpoint: z.string(),
  deepLink: z.string(),
  queue: QueueSchema,
  openDepth: z.number(),
});

const DecisionStatSchema = z
  .object({
    total: z.number(),
    medianSlaHours: z.number(),
  })
  .catchall(z.number());

const WindowSchema = z.object({
  places: DecisionStatSchema,
  communities: DecisionStatSchema,
  events: DecisionStatSchema,
});

const FunnelStageSchema = z.object({
  reported: z.number(),
  reviewed: z.number(),
  resolvedOrActioned: z.number(),
});

/**
 * Parses the committed `src/fixtures/admin-metrics.json` and nothing else, so
 * every key is required: the point of parsing a checked-in artifact is to fail
 * the build when it drifts. `generatedAt` defaulting to *now* was the worst of
 * it -- sample data with no stamp of its own claimed to have been generated the
 * moment the page was opened.
 */
export const AdminMetricsFixtureSchema = z.object({
  generatedAt: z.string(),
  surfaces: z.array(SurfaceSchema),
  decisions: z.object({ "7d": WindowSchema, "30d": WindowSchema }),
  trend: z.object({
    places: z.array(z.number()),
    communities: z.array(z.number()),
    events: z.array(z.number()),
  }),
  trendLabels: z.array(z.string()),
  funnel: z.object({
    places: FunnelStageSchema,
    communities: FunnelStageSchema,
    events: FunnelStageSchema,
  }),
});

export type AdminMetricsFixture = z.infer<typeof AdminMetricsFixtureSchema>;
export type Surface = z.infer<typeof SurfaceSchema>;
export type DecisionStat = z.infer<typeof DecisionStatSchema>;

export const FIXTURE: AdminMetricsFixture =
  AdminMetricsFixtureSchema.parse(fixtureJson);

/**
 * Required, because these three rows are being counted.
 *
 * `EventRecord` (catalyrst/ui3/src/generated/catalyst/events/EventRecord.ts) types all
 * three as plain booleans -- they are always on the wire. A default of `false`
 * meant a page of rows the parser did not understand still landed in the
 * denominator as "not approved", so the approved/featured tallies on the admin
 * metrics page moved without anything having been measured. An unreadable row
 * now aborts the whole count and `fetchLiveEventCounts` answers null, which its
 * caller already renders as "unavailable".
 */
const LiveEventRowSchema = z.object({
  approved: z.boolean(),
  rejected: z.boolean(),
  highlighted: z.boolean(),
});

export type LiveEventCounts = { approved: number; featured: number };

const EVENTS_PAGE_SIZE = 500;

export async function fetchLiveEventCounts(
  opts: GetOptions = {},
): Promise<LiveEventCounts | null> {
  try {
    let approved = 0;
    let featured = 0;
    let offset = 0;
    for (;;) {
      const env = await getJSON<Envelope<unknown[]>>("/events/api/events", {
        ...opts,
        query: { list: "all", limit: EVENTS_PAGE_SIZE, offset },
      });
      const rows = env.data;
      if (!Array.isArray(rows)) return null;
      for (const raw of rows) {
        const r = LiveEventRowSchema.safeParse(raw);
        if (!r.success) return null;
        if (r.data.highlighted) featured += 1;
        else if (r.data.approved && !r.data.rejected) approved += 1;
      }
      if (rows.length < EVENTS_PAGE_SIZE) break;
      offset += EVENTS_PAGE_SIZE;
    }
    return { approved, featured };
  } catch {
    return null;
  }
}

/*
 * The admin metrics page -- what actually ships.
 *
 * `admin-metrics.json` says in its own `_source` field that the counts are
 * synthetic. Rendered inside admin chrome they read as production telemetry,
 * so they do not ship as numbers. The rules, from the build gate:
 *
 *   - The two genuinely live counts (approved / featured events) come from
 *     GET /events/api/events?list=all -- a public endpoint
 *     (catalyrst-events/src/handlers/events.rs:345-362, `optional_user`). They
 *     are labelled "live - public events API".
 *   - Every other tile is an empty state carrying its reason. Not a zero, not
 *     a dash with a sparkline: a zero and a greyed chart still read as a
 *     measurement.
 *   - The fixture is not deleted. It is reachable only through
 *     `loadSampleAdminMetrics()`, which is off by default and must be rendered
 *     with a persistent "sample data" banner.
 *   - `operator-metrics.server.ts` is NOT used as a substitute. It belongs to
 *     the creator-hub workflow and it exposes aggregate telemetry to any
 *     visitor with no authorization at all -- that is a finding to report, not
 *     a data source to adopt.
 */

const NO_SOURCE = "No metrics source is wired on this node.";

const EVENTS_PUBLIC_CHECK =
  "catalyrst-events/src/handlers/events.rs:345-362 (optional_user, public)";

export type MetricTile =
  | {
      key: string;
      label: string;
      kind: "live";
      value: number;
      /** Provenance string the UI must render next to the number. */
      source: string;
    }
  | { key: string; label: string; kind: "unavailable"; reason: string };

export type AdminMetricsView = {
  generatedAt: string;
  /** Tiles that have a real source, plus explicit empty states for the rest. */
  tiles: MetricTile[];
  /** The whole KPI table: no aggregation endpoint exists. */
  kpis: Unavailable;
  /** Decision trend over time: no aggregation endpoint exists. */
  trend: Unavailable;
  /** Report -> review -> resolve funnel: no aggregation endpoint exists. */
  funnel: Unavailable;
};

function noSource(key: string, label: string): MetricTile {
  return { key, label, kind: "unavailable", reason: NO_SOURCE };
}

export async function loadAdminMetrics(
  opts: GetOptions = {},
): Promise<AdminMetricsView> {
  const liveEvents = await fetchLiveEventCounts(opts);

  const eventTiles: MetricTile[] = liveEvents
    ? [
        {
          key: "events.approved",
          label: "Approved events",
          kind: "live",
          value: liveEvents.approved,
          source: "live \u{B7} public events API",
        },
        {
          key: "events.featured",
          label: "Featured events",
          kind: "live",
          value: liveEvents.featured,
          source: "live \u{B7} public events API",
        },
      ]
    : [
        {
          key: "events.approved",
          label: "Approved events",
          kind: "unavailable",
          reason: "The public events API could not be read.",
        },
        {
          key: "events.featured",
          label: "Featured events",
          kind: "unavailable",
          reason: "The public events API could not be read.",
        },
      ];

  const unavailableTiles: MetricTile[] = [
    noSource("events.pending", "Events pending review"),
    noSource("places.open", "Open place reports"),
    noSource("places.decisions", "Place decisions"),
    noSource("communities.open", "Community reports"),
    noSource("communities.decisions", "Community decisions"),
    noSource("sla.median", "Median time to decision"),
  ];

  const block = (what: string): Unavailable =>
    unavailable("not-wired", `${what} \u{2014} ${NO_SOURCE}`, {
      serverCheck: null,
      fix:
        "Needs a real aggregation endpoint. The only live counts available today " +
        `are approved/featured events (${EVENTS_PUBLIC_CHECK}).`,
    });

  return {
    generatedAt: new Date().toISOString(),
    tiles: [...eventTiles, ...unavailableTiles],
    kpis: block("Moderation KPIs"),
    trend: block("Decision trend"),
    funnel: block("Moderation funnel"),
  };
}

/**
 * Synthetic layout data. Off by default and never called from a normal loader.
 * The caller MUST render a persistent banner while this is on: every number it
 * returns was invented by `src/fixtures/admin-metrics.json`.
 */
export type SampleAdminMetrics = {
  synthetic: true;
  banner: string;
  data: AdminMetricsFixture;
};

export function loadSampleAdminMetrics(): SampleAdminMetrics {
  return {
    synthetic: true,
    banner:
      "SAMPLE DATA \u{2014} every number on this page is synthetic, from " +
      "src/fixtures/admin-metrics.json. It is not telemetry.",
    data: AdminMetricsFixtureSchema.parse(fixtureJson),
  };
}
