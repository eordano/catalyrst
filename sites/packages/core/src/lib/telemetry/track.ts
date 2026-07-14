import type { TelemetryEventName, TelemetryEvents } from "./events";

export type TrackContext = {
  sid: string;
  story?: string;
  variant?: string;
  experimentKey?: string;
};

export type TrackFn = <K extends TelemetryEventName>(
  event: K,
  props: TelemetryEvents[K],
  ctx: TrackContext,
) => void;

function env(name: string): string | undefined {
  try {
    return typeof process !== "undefined" ? process.env?.[name] : undefined;
  } catch {
    return undefined;
  }
}

function publicEnv(name: string): string | undefined {
  try {
    const e = (import.meta as unknown as { env?: Record<string, string> }).env;
    if (e) return e[`VITE_${name}`] ?? e[name];
  } catch {
  }
  return undefined;
}

function runtimeEnv(name: string): string | undefined {
  if (typeof window === "undefined") return undefined;
  const pub = (window as unknown as { __DCL_PUBLIC__?: Record<string, string> })
    .__DCL_PUBLIC__;
  const v = pub?.[name];
  return v && v !== "" ? v : undefined;
}

function telemetryBase(): string | undefined {
  return env("TELEMETRY_URL") ?? runtimeEnv("TELEMETRY_URL") ?? publicEnv("TELEMETRY_URL");
}

const isBrowser = (): boolean => typeof window !== "undefined";

export type SegmentTrackBody = {
  type: "track";
  event: string;
  anonymousId: string;
  properties: Record<string, unknown> & {
    story?: string;
    variant?: string;
    exp_key?: string;
  };
};

export function buildSegmentBody(
  event: string,
  props: Record<string, unknown>,
  ctx: TrackContext,
): SegmentTrackBody {
  return {
    type: "track",
    event,
    anonymousId: ctx.sid,
    properties: {
      ...props,
      story: ctx.story,
      variant: ctx.variant,
      exp_key: ctx.experimentKey,
    },
  };
}

function sendToCatalyst(body: SegmentTrackBody): void {
  const base = telemetryBase();
  if (!base) return;

  const url = `${base.replace(/\/+$/, "")}/v1/track`;
  const payload = JSON.stringify(body);

  if (isBrowser()) {
    try {
      const beacon = navigator?.sendBeacon?.bind(navigator);
      if (beacon) {
        const ok = postTelemetry(url, payload);
        if (!ok) beacon(url, new Blob([payload], { type: "text/plain" }));
        return;
      }
    } catch {
    }
  }

  postTelemetry(url, payload);
}

function postTelemetry(url: string, payload: string): boolean {
  try {
    void fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: "Basic dcl-sites",
      },
      body: payload,
      keepalive: true,
    }).catch(() => {});
    return true;
  } catch {
    return false;
  }
}

// Dev-only advisory shape check. Production and test builds skip it entirely,
// so the contract is never bundled or fetched there; the authoritative
// enforcement lives at ingest (catalyrst-telemetry). Never throws, always lets
// the event send -- analytics must not break the app.
function devValidate(event: string, props: Record<string, unknown>): void {
  const env = (import.meta as { env?: { DEV?: boolean; TEST?: boolean } }).env;
  if (!env?.DEV || env.TEST) return;
  void import("./dev-validate")
    .then((m) => m.warnIfInvalid(event, props))
    .catch(() => {});
}

export function track<K extends TelemetryEventName>(
  event: K,
  props: TelemetryEvents[K],
  ctx: TrackContext,
): void {
  try {
    devValidate(event, props as Record<string, unknown>);
    sendToCatalyst(buildSegmentBody(event, props, ctx));
  } catch {
  }
}

export const EXPERIMENT_EXPOSED = "experiment_exposed";

export function trackExposure(ctx: TrackContext): void {
  track(
    EXPERIMENT_EXPOSED,
    { exp_key: ctx.experimentKey, variant: ctx.variant },
    ctx,
  );
}
