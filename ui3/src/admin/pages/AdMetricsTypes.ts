import type { ReactNode } from "react";

export type SurfaceKey = "places" | "communities" | "events";

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
