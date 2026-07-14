import type { ReactNode } from "react";

export type SurfaceKey = "places" | "communities" | "events";

export type Range = "7d" | "30d";

export type SurfaceKpi = {
  key: SurfaceKey;
  label: string;
  openDepth: number;
  decisions: number;
  approvedish: number;
  dismissedish: number;
  approvalRate: number;
  medianSlaHours: number;
  deepLink: string;
  live: boolean;
};

export type TrendSeries = {
  key: SurfaceKey;
  label: string;
  points: number[];
};

export type FunnelRow = {
  key: SurfaceKey;
  label: string;
  reported: number;
  reviewed: number;
  resolvedOrActioned: number;
};

export type MetricsLinkProps = {
  className?: string;
  to: string;
  prefetch?: "intent" | "render" | "none" | "viewport";
  onClick?: () => void;
  "aria-label"?: string;
  children?: ReactNode;
};

/**
 * A metric tile is either a real measurement carrying its provenance, or an
 * explicit empty state carrying its reason.
 *
 * There is deliberately no third shape. A zero and a greyed-out sparkline both
 * read as measurements to a moderator, which is how a bundled fixture came to
 * be rendered as production telemetry on this page.
 */
export type AdMetricTile =
  | {
      key: string;
      label: string;
      kind: "live";
      value: number;
      /** Provenance, rendered next to the number. */
      source: string;
    }
  | { key: string; label: string; kind: "unavailable"; reason: string };

/** An entire panel that has no data source on this node. */
export type AdMetricsBlock = {
  message: string;
  fix?: string;
  serverCheck?: string | null;
};

/** Static navigation into the moderation consoles. Not a measurement. */
export type AdMetricsSurfaceLink = {
  key: SurfaceKey;
  label: string;
  deepLink: string;
};
