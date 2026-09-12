import type { ComponentType, ReactNode } from "react";

export type ServerPanel<T> =
  | { ok: true; data: T }
  | { ok: false; message: string; fix?: string };

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
  actionables: string[];
  ageMs: number;
  recovered?: boolean;
};

export type ServerEnvRow = {
  name: string;
  purpose?: string;
  secret: boolean;
  fileValue: string | null;
  liveValue: string | null;
  liveInSites: boolean;
  pendingRestart: boolean;
};

export type ServerEnvData = {
  path: string;
  rows: ServerEnvRow[];
  preservedLines: number;
};

export type ServerNotice = { ok: boolean; message: string };

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
