import type { OverlayPush } from "../../generated/bridge/OverlayPush";
import type { SignedFetchPayload } from "../../generated/bridge/SignedFetchPayload";

export class CatalystError extends Error {
  override name = "CatalystError";
  status: number;
  url: string;

  constructor(message: string, url: string, status = 0) {
    super(message);
    this.status = status;
    this.url = url;
  }
}

export type QueryParams = Record<string, string | number | boolean | null | undefined>;

export type RequestOpts = {
  base?: string;
  service?: ServiceName;
  query?: QueryParams;
  signal?: AbortSignal;
  headers?: Record<string, string>;
  fetchImpl?: typeof fetch;
};

export type SendOpts = RequestOpts & {
  method?: string;
  body?: unknown;
  timeoutMs?: number;
};

type CatalystWindow = Window & typeof globalThis & { __CATALYST_BASE__?: string };

function errMessage(err: unknown, fallback: string): string {
  return (err as { message?: string } | null)?.message ?? fallback;
}

const DEFAULT_BASE = "https://catalyst.example.com";

// The overlay is a client-only build served from the origin that also fronts
// the catalyst API, so the served origin is the portable default. DEFAULT_BASE
// is reached only in SSR/non-browser bundles: import.meta.env.SSR is statically
// false in the client build, folding this branch (and the baked host) away.
function defaultBase(): string {
  return import.meta.env.SSR ? DEFAULT_BASE : window.location.origin;
}

export function catalystBase(override?: string): string {
  const fromEnv: string | undefined =
    typeof import.meta !== "undefined" ? import.meta.env?.VITE_CATALYST_URL : undefined;
  const fromWindow =
    typeof window !== "undefined" ? (window as CatalystWindow).__CATALYST_BASE__ : undefined;
  const base = override ?? fromEnv ?? fromWindow ?? defaultBase();
  return base.replace(/\/$/, "");
}

export type ServiceName =
  | "places"
  | "events"
  | "communities"
  | "communitiesCdn"
  | "notifications"
  | "badges"
  | "cameraReel"
  | "map"
  | "satellite";

// One line per service so a deployment (or the standalone build) can repoint
// each backend independently; the default fronts them all on the serving
// origin (see defaultBase) under the path prefixes the catalyst router exposes.
const SERVICE_PATHS: Record<ServiceName, string> = {
  places: "/places",
  events: "/events",
  communities: "",
  communitiesCdn: "",
  notifications: "",
  badges: "",
  cameraReel: "/camera-reel",
  map: "",
  satellite: "/satellite",
};

const SERVICE_ENV_KEYS: Record<ServiceName, string> = {
  places: "VITE_PLACES_URL",
  events: "VITE_EVENTS_URL",
  communities: "VITE_COMMUNITIES_URL",
  communitiesCdn: "VITE_COMMUNITIES_CDN_URL",
  notifications: "VITE_NOTIFICATIONS_URL",
  badges: "VITE_BADGES_URL",
  cameraReel: "VITE_CAMERA_REEL_URL",
  map: "VITE_MAP_URL",
  satellite: "VITE_SATELLITE_URL",
};

type ServicesWindow = Window &
  typeof globalThis & { __SERVICE_BASES__?: Partial<Record<ServiceName, string>> };

export function serviceBase(service: ServiceName, override?: string): string {
  const fromEnv: string | undefined =
    typeof import.meta !== "undefined"
      ? (import.meta.env?.[SERVICE_ENV_KEYS[service]] as string | undefined)
      : undefined;
  const fromWindow =
    typeof window !== "undefined"
      ? (window as ServicesWindow).__SERVICE_BASES__?.[service]
      : undefined;
  const base = override ?? fromEnv ?? fromWindow ?? `${defaultBase()}${SERVICE_PATHS[service]}`;
  return base.replace(/\/$/, "");
}

export function servicePath(service: ServiceName, override?: string): string {
  const fromEnv: string | undefined =
    typeof import.meta !== "undefined"
      ? (import.meta.env?.[SERVICE_ENV_KEYS[service]] as string | undefined)
      : undefined;
  const base = override ?? fromEnv ?? SERVICE_PATHS[service];
  return base.replace(/\/$/, "");
}

function resolveBase(opts: RequestOpts): string {
  if (opts.base != null) return catalystBase(opts.base);
  if (opts.service) return serviceBase(opts.service);
  return catalystBase();
}

export function buildQuery(query?: QueryParams): string {
  if (!query) return "";
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value === undefined || value === null || value === "") continue;
    params.set(key, String(value));
  }
  const qs = params.toString();
  return qs ? `?${qs}` : "";
}

export async function getJSON<T = unknown>(path: string, opts: RequestOpts = {}): Promise<T> {
  const base = resolveBase(opts);
  const cleanPath = path.startsWith("/") ? path : `/${path}`;
  const url = `${base}${cleanPath}${buildQuery(opts.query)}`;
  const doFetch = opts.fetchImpl ?? fetch;

  let res: Response;
  try {
    res = await doFetch(url, {
      signal: opts.signal,
      headers: { accept: "application/json", ...(opts.headers ?? {}) },
    });
  } catch (err) {
    throw new CatalystError(
      `Catalyst request failed: ${errMessage(err, "network error")}`,
      url,
    );
  }

  if (!res.ok) {
    throw new CatalystError(
      `Catalyst returned ${res.status} ${res.statusText}`,
      url,
      res.status,
    );
  }

  try {
    return (await res.json()) as T;
  } catch (err) {
    throw new CatalystError(
      `Catalyst returned invalid JSON: ${errMessage(err, "parse error")}`,
      url,
      res.status,
    );
  }
}

export async function sendJSON<T = unknown>(path: string, opts: SendOpts = {}): Promise<T | null> {
  const base = resolveBase(opts);
  const cleanPath = path.startsWith("/") ? path : `/${path}`;
  const url = `${base}${cleanPath}${buildQuery(opts.query)}`;
  const doFetch = opts.fetchImpl ?? fetch;
  const method = (opts.method ?? "POST").toUpperCase();
  const hasBody = opts.body !== undefined && opts.body !== null;

  let res: Response;
  try {
    res = await doFetch(url, {
      method,
      signal: opts.signal,
      headers: {
        accept: "application/json",
        ...(hasBody ? { "content-type": "application/json" } : {}),
        ...(opts.headers ?? {}),
      },
      body: hasBody ? JSON.stringify(opts.body) : undefined,
    });
  } catch (err) {
    throw new CatalystError(
      `Catalyst request failed: ${errMessage(err, "network error")}`,
      url,
    );
  }

  if (!res.ok) {
    let serverMsg = "";
    try {
      const txt = await res.text();
      if (txt) {
        try {
          serverMsg = (JSON.parse(txt) as { message?: string } | null)?.message ?? "";
        } catch {
          serverMsg = txt;
        }
      }
    } catch {
    }
    throw new CatalystError(
      serverMsg || `Catalyst returned ${res.status} ${res.statusText}`,
      url,
      res.status,
    );
  }

  if (res.status === 204) return null;
  let text: string;
  try {
    text = await res.text();
  } catch {
    return null;
  }
  if (!text) return null;
  try {
    return JSON.parse(text) as T;
  } catch {
    return text as unknown as T;
  }
}

type SignedFetchOpts = { method?: string; body?: unknown; timeoutMs?: number };

type SignedFetchResult = { status: number; body: string };

type SignedFetchPush = Partial<
  Extract<OverlayPush, { kind: "signedFetchResult" }>
>;

function isSignedFetchPush(v: unknown): v is SignedFetchPush {
  return !!v && typeof v === "object";
}

export function signedFetch(url: string, opts: SignedFetchOpts = {}): Promise<SignedFetchResult> {
  return new Promise<SignedFetchResult>((resolve, reject) => {
    const bridge = typeof window !== "undefined" ? window.dclBridge : undefined;
    const send = bridge?.send;
    const onState = bridge?.onState;
    if (!bridge || typeof send !== "function" || typeof onState !== "function") {
      reject(
        new CatalystError("Signed fetch unavailable: engine bridge not ready", url),
      );
      return;
    }

    const id =
      typeof crypto !== "undefined" && crypto.randomUUID
        ? crypto.randomUUID()
        : `sf-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const method = (opts.method ?? "POST").toUpperCase();
    const hasBody = opts.body !== undefined && opts.body !== null;
    const body = hasBody
      ? typeof opts.body === "string"
        ? opts.body
        : JSON.stringify(opts.body)
      : undefined;

    let unsubscribe: () => void = () => {};
    let timer: ReturnType<typeof setTimeout> | null = null;
    const settle = <A>(fn: (arg: A) => void, arg: A) => {
      try {
        unsubscribe();
      } catch {
      }
      if (timer) clearTimeout(timer);
      fn(arg);
    };

    unsubscribe = onState((push) => {
      if (
        isSignedFetchPush(push) &&
        push.kind === "signedFetchResult" &&
        push.id === id
      ) {
        settle(resolve, { status: push.status ?? 0, body: push.body ?? "" });
      }
    });

    const timeoutMs = opts.timeoutMs ?? 30000;
    timer = setTimeout(() => {
      settle(reject, new CatalystError("Signed fetch timed out", url));
    }, timeoutMs);

    const payload: SignedFetchPayload = { id, url, method, body };
    send("SignedFetch", payload);
  });
}

export async function sendSignedJSON<T = unknown>(
  path: string,
  opts: SendOpts = {},
): Promise<T | null> {
  const base = resolveBase(opts);
  const cleanPath = path.startsWith("/") ? path : `/${path}`;
  const url = `${base}${cleanPath}${buildQuery(opts.query)}`;

  const { status, body } = await signedFetch(url, {
    method: opts.method ?? "POST",
    body: opts.body,
    timeoutMs: opts.timeoutMs,
  });

  if (status < 200 || status >= 300) {
    let serverMsg = "";
    if (body) {
      try {
        serverMsg = (JSON.parse(body) as { message?: string } | null)?.message ?? "";
      } catch {
        serverMsg = body;
      }
    }
    throw new CatalystError(serverMsg || `Catalyst returned ${status}`, url, status);
  }

  if (status === 204 || !body) return null;
  try {
    return JSON.parse(body) as T;
  } catch {
    return body as unknown as T;
  }
}
