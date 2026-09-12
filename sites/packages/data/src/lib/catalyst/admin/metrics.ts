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

const NO_SOURCE = "No metrics source is wired on this node.";

const EVENTS_PUBLIC_CHECK =
  "catalyrst-events/src/handlers/events.rs:345-362 (optional_user, public)";

export type MetricTile =
  | {
      key: string;
      label: string;
      kind: "live";
      value: number;
      source: string;
    }
  | { key: string; label: string; kind: "unavailable"; reason: string };

export type AdminMetricsView = {
  generatedAt: string;
  tiles: MetricTile[];
  kpis: Unavailable;
  trend: Unavailable;
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
