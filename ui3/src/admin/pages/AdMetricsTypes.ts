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

export type AdMetricTile =
  | {
      key: string;
      label: string;
      kind: "live";
      value: number;
      source: string;
    }
  | { key: string; label: string; kind: "unavailable"; reason: string };

export type AdMetricsBlock = {
  message: string;
  fix?: string;
  serverCheck?: string | null;
};

export type AdMetricsSurfaceLink = {
  key: SurfaceKey;
  label: string;
  deepLink: string;
};
