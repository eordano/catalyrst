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

function telemetryBase(): string | undefined {
  return env("TELEMETRY_URL") ?? publicEnv("TELEMETRY_URL");
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
        const ok = tryKeepaliveFetch(url, payload);
        if (!ok) beacon(url, new Blob([payload], { type: "text/plain" }));
        return;
      }
    } catch {
    }
  }

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
  } catch {
  }
}

function tryKeepaliveFetch(url: string, payload: string): boolean {
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

export function track<K extends TelemetryEventName>(
  event: K,
  props: TelemetryEvents[K],
  ctx: TrackContext,
): void {
  try {
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
