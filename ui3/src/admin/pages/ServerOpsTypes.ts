import type { ComponentType, ReactNode } from "react";

export type ServerPanel<T> =
  | { ok: true; data: T }
  | { ok: false; message: string; fix?: string };

/** "off": the node's config does not enable this service; it is not probed. */
export type ServerServiceState = "ok" | "answering" | "down" | "off";

export type ServerServiceRow = {
  key: string;
  name: string;
  unit: string;
  port: number;
  url: string;
  serves: string;
  state: ServerServiceState;
  httpStatus: number;
  latencyMs: number;
  detail: string;
  /** Ordered operator remedies; entries starting with "$ " render as commands. */
  actionables: string[];
  /** Ms since this row's result was taken; 0 means this request probed it. */
  ageMs: number;
  /** The row was unhealthy and a live recheck just watched it come back. */
  recovered?: boolean;
};

export type ServerEnvRow = {
  name: string;
  purpose?: string;
  secret: boolean;
  /** Value persisted in the operator env file; null when not in the file. */
  fileValue: string | null;
  /** Value the sites process runs with; null when unset or secret. */
  liveValue: string | null;
  /** The sites process currently sees this variable (even when secret). */
  liveInSites: boolean;
  /** Persisted value differs from what the sites process is running with. */
  pendingRestart: boolean;
};

export type ServerEnvData = {
  path: string;
  rows: ServerEnvRow[];
  preservedLines: number;
};

export type ServerNotice = { ok: boolean; message: string };

/** Live auto-recheck of unhealthy services, driven by the consuming route. */
export type ServerWatch = {
  intervalMs: number;
  checking: boolean;
};

export type ServerPendingEnv = { name: string; intent: "env-save" | "env-delete" };

export type ServerDisk = {
  path: string;
  totalBytes: number;
  freeBytes: number;
  usedPercent: number;
};

export type OperatorFormProps = {
  method: "get" | "post";
  children: ReactNode;
  className?: string;
};

export type ServerOpsPageProps = {
  services: ServerServiceRow[];
  env: ServerPanel<ServerEnvData>;
  authMode: "wallet" | "edge";
  setupHref?: string;
  disk?: ServerDisk | null;
  notice?: ServerNotice | null;
  watch?: ServerWatch | null;
  recheckingAll?: boolean;
  recheckingKey?: string | null;
  pendingEnv?: ServerPendingEnv | null;
  FormComponent?: ComponentType<OperatorFormProps>;
};
