function env(name: string): string | undefined {
  try {
    return typeof process !== "undefined" ? process.env?.[name] : undefined;
  } catch {
    return undefined;
  }
}

export type RuntimeFlag = {
  killed?: boolean;
  variant?: string;
  flags?: Record<string, unknown>;
};

export type ForcedFlag = {
  value: boolean;
  variant?: string;
  overridden: boolean;
};

function coerceFlag(value: unknown): RuntimeFlag | null {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  const v = value as Record<string, unknown>;
  const out: RuntimeFlag = {};
  if (v.killed === true || v.enabled === false) out.killed = true;
  if (typeof v.variant === "string") out.variant = v.variant;
  else if (typeof v.override === "string") out.variant = v.override;
  if (
    v.flags &&
    typeof v.flags === "object" &&
    !Array.isArray(v.flags) &&
    Object.keys(v.flags as Record<string, unknown>).length > 0
  ) {
    out.flags = v.flags as Record<string, unknown>;
  }
  if (!out.killed && out.variant === undefined && !out.flags) {
    return null;
  }
  return out;
}

export type FlagOpts = {
  signal?: AbortSignal;
  timeoutMs?: number;
  user?: string;
};

const FLAGS_TTL_MS = 15_000;
const flagCache = new Map<string, { at: number; value: RuntimeFlag | null }>();
const flagInflight = new Map<string, Promise<RuntimeFlag | null>>();

const forcedCache = new Map<
  string,
  { at: number; value: Record<string, ForcedFlag> | null }
>();
const forcedInflight = new Map<string, Promise<Record<string, ForcedFlag> | null>>();

function cacheKey(...parts: (string | undefined)[]): string {
  return parts.map((p) => p ?? "").join("\u0000");
}

function userParam(user: string | undefined, sep: string): string {
  return user ? `${sep}user=${encodeURIComponent(user)}` : "";
}

export function resetRuntimeFlagCache(): void {
  flagCache.clear();
  flagInflight.clear();
  forcedCache.clear();
  forcedInflight.clear();
}

export async function getRuntimeFlags(
  experimentKey: string,
  opts: FlagOpts = {},
): Promise<RuntimeFlag | null> {
  const base = env("TELEMETRY_URL");
  if (!base) return null;

  const key = cacheKey(experimentKey, opts.user);
  const cached = flagCache.get(key);
  if (cached && Date.now() - cached.at < FLAGS_TTL_MS) return cached.value;
  const inflight = flagInflight.get(key);
  if (inflight) return inflight;

  const promise = fetchRuntimeFlags(base, experimentKey, opts)
    .then((value) => {
      flagCache.set(key, { at: Date.now(), value });
      return value;
    })
    .finally(() => {
      flagInflight.delete(key);
    });
  flagInflight.set(key, promise);
  return promise;
}

async function fetchRuntimeFlags(
  base: string,
  experimentKey: string,
  opts: FlagOpts = {},
): Promise<RuntimeFlag | null> {
  const url = `${base.replace(/\/$/, "")}/dash/experiments?key=${encodeURIComponent(
    experimentKey,
  )}${userParam(opts.user, "&")}`;
  const timeoutMs = opts.timeoutMs ?? 1500;

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  if (opts.signal) {
    if (opts.signal.aborted) controller.abort();
    else opts.signal.addEventListener("abort", () => controller.abort(), { once: true });
  }

  try {
    const res = await fetch(url, {
      headers: { Accept: "application/json" },
      signal: controller.signal,
    });
    if (!res.ok) return null;
    const payload: unknown = await res.json();
    return coerceFlag(payload);
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

function coerceForcedFlags(payload: unknown): Record<string, ForcedFlag> | null {
  if (payload == null || typeof payload !== "object" || Array.isArray(payload)) {
    return null;
  }
  const flags = (payload as Record<string, unknown>).flags;
  if (flags == null || typeof flags !== "object" || Array.isArray(flags)) {
    return null;
  }
  const out: Record<string, ForcedFlag> = {};
  for (const [name, raw] of Object.entries(flags as Record<string, unknown>)) {
    if (raw == null || typeof raw !== "object" || Array.isArray(raw)) continue;
    const r = raw as Record<string, unknown>;
    const entry: ForcedFlag = {
      value: r.value === true,
      overridden: r.overridden === true,
    };
    if (typeof r.variant === "string") entry.variant = r.variant;
    out[name] = entry;
  }
  return out;
}

async function fetchForcedFlags(
  base: string,
  opts: FlagOpts,
): Promise<Record<string, ForcedFlag> | null> {
  const url = `${base.replace(/\/$/, "")}/dash/flags${userParam(opts.user, "?")}`;
  const timeoutMs = opts.timeoutMs ?? 1500;

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  if (opts.signal) {
    if (opts.signal.aborted) controller.abort();
    else opts.signal.addEventListener("abort", () => controller.abort(), { once: true });
  }

  try {
    const res = await fetch(url, {
      headers: { Accept: "application/json" },
      signal: controller.signal,
    });
    if (!res.ok) return null;
    const payload: unknown = await res.json();
    return coerceForcedFlags(payload);
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

export async function getForcedFlags(
  opts: FlagOpts = {},
): Promise<Record<string, ForcedFlag> | null> {
  const base = env("TELEMETRY_URL");
  if (!base) return null;

  const key = cacheKey(opts.user);
  const cached = forcedCache.get(key);
  if (cached && Date.now() - cached.at < FLAGS_TTL_MS) return cached.value;
  const inflight = forcedInflight.get(key);
  if (inflight) return inflight;

  const promise = fetchForcedFlags(base, opts)
    .then((value) => {
      forcedCache.set(key, { at: Date.now(), value });
      return value;
    })
    .finally(() => {
      forcedInflight.delete(key);
    });
  forcedInflight.set(key, promise);
  return promise;
}

export async function resolveFlag(
  name: string,
  fallback: boolean,
  opts: FlagOpts = {},
): Promise<boolean> {
  try {
    const flags = await getForcedFlags(opts);
    const forced = flags?.[name];
    if (forced?.overridden) return forced.value;
  } catch {
  }
  return fallback;
}

// Runtime activation for a draft experiment: ON only when an override row with
// a non-empty payload exists and is not killed. A no-op row (the /flags UI form
// saved with nothing set) coerces to null upstream and deliberately stays OFF,
// as does an unreachable service (fail-closed to draft). envActive is the
// legacy env-var activation and wins without fetching.
export async function experimentActive(
  experimentKey: string,
  opts: FlagOpts & { envActive?: boolean } = {},
): Promise<boolean> {
  if (opts.envActive) return true;
  try {
    const flag = await getRuntimeFlags(experimentKey, opts);
    return flag !== null && flag.killed !== true;
  } catch {
    return false;
  }
}
