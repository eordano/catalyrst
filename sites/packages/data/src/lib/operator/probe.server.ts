import type { HealthExpectation, OperatorService } from "./registry";

export type ProbeState = "ok" | "answering" | "down";

export type ServiceProbe = {
  key: string;
  url: string;
  state: ProbeState;
  /** null when no HTTP response arrived at all (refused/timeout) -- a real
   *  status can never be confused with the absence of one. */
  httpStatus: number | null;
  latencyMs: number;
  detail: string;
};

const PROBE_TIMEOUT_MS = 1500;

export function probeHost(): string {
  const h = process.env.OPERATOR_PROBE_HOST;
  return h && h.trim() !== "" ? h.trim() : "127.0.0.1";
}

function classify(status: number, expect: HealthExpectation): ProbeState {
  if (status >= 200 && status < 300) return "ok";
  return expect === "any-http" ? "ok" : "answering";
}

function describeError(e: unknown): string {
  const cause = (e as { cause?: { code?: string } })?.cause;
  if (cause?.code === "ECONNREFUSED") return "connection refused \u{2014} nothing is listening";
  if ((e as Error)?.name === "TimeoutError" || (e as { code?: string })?.code === "ABORT_ERR") {
    return `no answer within ${PROBE_TIMEOUT_MS}ms`;
  }
  const msg = e instanceof Error ? e.message : String(e);
  return msg || "unreachable";
}

export async function probeService(
  service: OperatorService,
  signal?: AbortSignal,
): Promise<ServiceProbe> {
  const url = `http://${probeHost()}:${service.port}${service.healthPath}`;
  const started = performance.now();
  const timeout = AbortSignal.timeout(PROBE_TIMEOUT_MS);
  const combined = signal ? AbortSignal.any([signal, timeout]) : timeout;
  try {
    const res = await fetch(url, { redirect: "manual", signal: combined });
    await res.body?.cancel();
    const latencyMs = Math.round(performance.now() - started);
    const state = classify(res.status, service.expect);
    return {
      key: service.key,
      url,
      state,
      httpStatus: res.status,
      latencyMs,
      detail:
        state === "ok"
          ? `HTTP ${res.status} in ${latencyMs}ms`
          : `answers, but ${service.healthPath} returned HTTP ${res.status}`,
    };
  } catch (e) {
    return {
      key: service.key,
      url,
      state: "down",
      httpStatus: null,
      latencyMs: Math.round(performance.now() - started),
      detail: describeError(e),
    };
  }
}

export async function probeServices(
  services: OperatorService[],
  signal?: AbortSignal,
): Promise<ServiceProbe[]> {
  return Promise.all(services.map((s) => probeService(s, signal)));
}

export type SnapshotProbe = ServiceProbe & { probedAt: number };

const snapshot = new Map<string, SnapshotProbe>();

export function clearProbeSnapshot(): void {
  snapshot.clear();
}

/**
 * Probe with a per-service scope: `only` re-probes those services and answers
 * the rest from the last snapshot (still probing any service never seen, so a
 * fresh process cannot serve holes). Each row carries `probedAt` so the UI
 * can say how old a cached answer is instead of implying "just now".
 */
export async function probeSnapshot(
  services: OperatorService[],
  opts: { only?: string[]; signal?: AbortSignal } = {},
): Promise<SnapshotProbe[]> {
  const only = opts.only;
  const toProbe =
    only && only.length > 0
      ? services.filter((s) => only.includes(s.key) || !snapshot.has(s.key))
      : services;
  const fresh = await probeServices(toProbe, opts.signal);
  const now = Date.now();
  for (const p of fresh) snapshot.set(p.key, { ...p, probedAt: now });
  return services.flatMap((s) => {
    const p = snapshot.get(s.key);
    return p ? [p] : [];
  });
}
