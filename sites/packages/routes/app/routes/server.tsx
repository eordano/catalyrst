import { useEffect, useRef, useState } from "react";
import { Form, useFetcher, useNavigation } from "react-router";

import ServerOpsPage from "@ui/admin/pages/ServerOpsPage";
import type {
  ServerEnvRow,
  ServerPanel,
  ServerEnvData,
  ServerPendingEnv,
  ServerServiceRow,
} from "@ui/admin/pages/ServerOpsTypes";
import SitesChrome from "@ui/web/frames/SitesChrome";

import { readVerifiedWallet } from "@core/lib/experiments/assign";
import {
  type OperatorEnvFile,
  readOperatorEnv,
  removeOperatorEnv,
  upsertOperatorEnv,
} from "@data/lib/operator/env-store.server";
import { diskStatus } from "@data/lib/operator/host.server";
import { probeSnapshot } from "@data/lib/operator/probe.server";
import { KNOWN_ENV, SERVICES, isSecretName, knownEnv } from "@data/lib/operator/registry";
import type { ControlResult } from "@data/lib/catalyst/admin/availability";

import type { Route } from "./+types/server";

const WATCH_INTERVAL_MS = 5000;

export function meta() {
  return [{ title: "Server" }];
}

type OperatorAuth =
  | { authorized: true; mode: "wallet" | "edge" }
  | { authorized: false; reason: string };

/**
 * With ADMIN_WALLETS unset the page trusts the edge alone: the exported nixos
 * module only routes /server through the nginx superadmin CIDR gate (loopback
 * by default), so a fresh unconfigured node still reaches its own operator
 * page. Setting ADMIN_WALLETS upgrades every request to wallet auth.
 */
function operatorAuthorized(request: Request): OperatorAuth {
  const raw = process.env.ADMIN_WALLETS;
  const wallets = (raw ?? "")
    .split(",")
    .map((w) => w.trim().toLowerCase())
    .filter(Boolean);
  if (wallets.length === 0) return { authorized: true, mode: "edge" };
  const wallet = readVerifiedWallet(request);
  if (wallet && wallets.includes(wallet.toLowerCase())) {
    return { authorized: true, mode: "wallet" };
  }
  return {
    authorized: false,
    reason: wallet
      ? "This wallet is not in ADMIN_WALLETS."
      : "ADMIN_WALLETS is set, so this page needs a signed-in operator wallet.",
  };
}

/** facts.nix service keys this node enables; null (unset, e.g. dev) means all. */
function enabledKeys(): Set<string> | null {
  const raw = process.env.CATALYRST_ENABLED_SERVICES;
  if (!raw || raw.trim() === "") return null;
  return new Set(
    raw
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean),
  );
}

function actionablesFor(s: (typeof SERVICES)[number], state: string): string[] {
  if (state === "down") {
    return [`$ systemctl status ${s.unit}`, `$ journalctl -u ${s.unit} -n 50 --no-pager`];
  }
  if (state === "answering") {
    return [
      `$ curl -i http://127.0.0.1:${s.port}${s.healthPath}`,
      `$ journalctl -u ${s.unit} -n 50 --no-pager`,
    ];
  }
  return [];
}

function envPanel(file: ControlResult<OperatorEnvFile>): ServerPanel<ServerEnvData> {
  if (!file.ok) return { ok: false, message: file.message, fix: file.fix };
  const fileByName = new Map(file.data.entries.map((e) => [e.name, e.value]));
  const names = [...new Set([...KNOWN_ENV.map((v) => v.name), ...fileByName.keys()])].sort();
  const rows: ServerEnvRow[] = names.map((name) => {
    const secret = isSecretName(name);
    const inFile = fileByName.has(name);
    const fileValue = inFile ? (fileByName.get(name) as string) : null;
    const live = process.env[name];
    return {
      name,
      purpose: knownEnv(name)?.purpose,
      secret,
      fileValue: fileValue === null ? null : secret ? "" : fileValue,
      liveValue: live !== undefined && !secret ? live : null,
      liveInSites: live !== undefined,
      pendingRestart: fileValue !== null && live !== fileValue,
    };
  });
  return {
    ok: true,
    data: { path: file.data.path, rows, preservedLines: file.data.preservedLines },
  };
}

export async function loader({ request }: Route.LoaderArgs) {
  const auth = operatorAuthorized(request);
  if (!auth.authorized) {
    return { authorized: false as const, reason: auth.reason };
  }

  const only = (new URL(request.url).searchParams.get("recheck") ?? "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  const enabled = enabledKeys();
  const isEnabled = (s: (typeof SERVICES)[number]) =>
    enabled === null || (s.members ?? [s.key]).some((k) => enabled.has(k));
  const enabledServices = SERVICES.filter(isEnabled);

  const [probes, envFile, disk] = await Promise.all([
    probeSnapshot(enabledServices, {
      only: only.length > 0 ? only : undefined,
      signal: request.signal,
    }),
    readOperatorEnv(),
    diskStatus(),
  ]);
  const byKey = new Map(probes.map((p) => [p.key, p]));
  const now = Date.now();

  const services: ServerServiceRow[] = SERVICES.map((s) => {
    const off = !isEnabled(s);
    const p = byKey.get(s.key);
    const state = off ? ("off" as const) : (p?.state ?? ("down" as const));
    return {
      key: s.key,
      name: s.name,
      unit: s.unit,
      port: s.port,
      url: p?.url ?? `http://127.0.0.1:${s.port}${s.healthPath}`,
      serves: s.serves,
      state,
      httpStatus: p?.httpStatus ?? 0,
      latencyMs: p?.latencyMs ?? 0,
      detail: off
        ? "not enabled by this node's configuration"
        : (p?.detail ?? "not probed"),
      actionables: off ? [] : actionablesFor(s, state),
      ageMs: p ? Math.max(0, now - p.probedAt) : 0,
    };
  });

  return {
    authorized: true as const,
    mode: auth.mode,
    services,
    env: envPanel(envFile),
    disk,
  };
}

export async function action({ request }: Route.ActionArgs) {
  const auth = operatorAuthorized(request);
  if (!auth.authorized) {
    return { ok: false, message: auth.reason };
  }
  if (request.headers.get("sec-fetch-site") === "cross-site") {
    return { ok: false, message: "Cross-site writes are refused." };
  }

  const form = await request.formData();
  const intent = String(form.get("intent") ?? "");
  const name = String(form.get("name") ?? "").trim();
  const value = String(form.get("value") ?? "");

  if (intent === "env-delete") {
    const r = await removeOperatorEnv(name);
    return r.ok
      ? {
          ok: true,
          message: `${name} removed. Restart the consuming service to apply (this page's own process: systemctl restart catalyrst-sites).`,
        }
      : { ok: false, message: r.message };
  }
  if (intent === "env-save") {
    if (isSecretName(name) && value === "") {
      return {
        ok: false,
        message: `${name} is treated as a secret; enter a value to set or replace it.`,
      };
    }
    const r = await upsertOperatorEnv(name, value);
    return r.ok
      ? {
          ok: true,
          message: `${name} saved. Restart the consuming service to apply (this page's own process: systemctl restart catalyrst-sites).`,
        }
      : { ok: false, message: r.message };
  }
  return { ok: false, message: `Unknown intent ${intent || "(none)"}.` };
}

function pendingEnvFrom(formData: FormData | undefined): ServerPendingEnv | null {
  if (!formData) return null;
  const intent = formData.get("intent");
  const name = formData.get("name");
  if (typeof name !== "string" || name === "") return null;
  if (intent !== "env-save" && intent !== "env-delete") return null;
  return { name, intent };
}

export default function ServerRoute({ loaderData, actionData }: Route.ComponentProps) {
  const navigation = useNavigation();
  const fetcher = useFetcher<typeof loader>();

  const data = fetcher.data?.authorized ? fetcher.data : loaderData;

  const unhealthyKeys = data.authorized
    ? data.services
        .filter((s) => s.state === "down" || s.state === "answering")
        .map((s) => s.key)
    : [];
  const unhealthyCsv = unhealthyKeys.join(",");

  const wasUnhealthy = useRef<Set<string>>(new Set());
  const [recovered, setRecovered] = useState<string[]>([]);
  useEffect(() => {
    const now = new Set(unhealthyKeys);
    const back = [...wasUnhealthy.current].filter((k) => !now.has(k));
    if (back.length > 0) setRecovered((r) => [...new Set([...r, ...back])]);
    wasUnhealthy.current = now;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [unhealthyCsv]);

  useEffect(() => {
    if (unhealthyCsv === "") return;
    const id = setInterval(() => {
      if (document.hidden) return;
      if (fetcher.state !== "idle" || navigation.state !== "idle") return;
      fetcher.load(`/server?recheck=${encodeURIComponent(unhealthyCsv)}`);
    }, WATCH_INTERVAL_MS);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [unhealthyCsv, navigation.state]);

  if (!data.authorized) {
    return (
      <SitesChrome>
        <main style={{ maxWidth: 640, margin: "0 auto", padding: "48px 20px" }}>
          <h1>Server</h1>
          <p>{data.reason}</p>
        </main>
      </SitesChrome>
    );
  }

  const pendingSearch = navigation.location
    ? new URLSearchParams(navigation.location.search)
    : null;
  const navRecheck = pendingSearch?.get("recheck") ?? null;
  const services = data.services.map((s) =>
    s.state === "ok" && recovered.includes(s.key) ? { ...s, recovered: true } : s,
  );

  return (
    <SitesChrome>
      <ServerOpsPage
        services={services}
        env={data.env}
        authMode={data.mode}
        setupHref="/server/setup"
        disk={data.disk}
        notice={actionData ?? null}
        watch={{ intervalMs: WATCH_INTERVAL_MS, checking: fetcher.state !== "idle" }}
        recheckingAll={navigation.state !== "idle" && !navRecheck && !navigation.formData}
        recheckingKey={navRecheck}
        pendingEnv={pendingEnvFrom(navigation.formData ?? undefined)}
        FormComponent={Form}
      />
    </SitesChrome>
  );
}
