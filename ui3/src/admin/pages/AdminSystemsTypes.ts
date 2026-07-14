import type { ComponentType } from "react";

export type Panel<T> =
  | { ok: true; data: T }
  | { ok: false; message: string; fix?: string };

export type SystemUnit = {
  unit: string;
  active_state: string;
  sub_state: string;
  n_restarts: number;
  active_since: string;
};

export type SystemProbe = {
  name: string;
  url: string;
  http_status: number;
  ok: boolean;
};

export type SystemLink = {
  label: string;
  href: string;
  scope: "public" | "operator";
};

export type SystemsData = {
  collectedAt: string;
  stale: boolean;
  units: SystemUnit[];
  probes: SystemProbe[];
};

export type DeployAction = {
  entityType: string;
  entityId: string;
  deployer: string;
  at: string;
};

export type ExperimentRow = {
  exp_key: string;
  exposures: number;
  variants: string[];
  metrics: { event: string; count: number }[];
  control?: string;
  reason?: string;
};

export type ExperimentsData = {
  readable: ExperimentRow[];
  unreadable: ExperimentRow[];
};

export type AdminLinkProps = {
  to: string;
  children: React.ReactNode;
  className?: string;
};

export type AdminSystemsPageProps = {
  systems: Panel<SystemsData>;
  links: SystemLink[];
  actions: Panel<DeployAction[]>;
  experiments: Panel<ExperimentsData>;
  now: number;
  LinkComponent?: ComponentType<AdminLinkProps>;
};
