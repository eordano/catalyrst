import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  buildSegmentBody,
  EXPERIMENT_EXPOSED,
  track,
  trackExposure,
  type TrackContext,
} from "./track";

const CTX: TrackContext = {
  sid: "sid-123",
  story: "homepage-hero",
  variant: "B",
  experimentKey: "hero_copy",
};

const trackAny = track as unknown as (
  event: string,
  props: Record<string, unknown>,
  ctx: TrackContext,
) => void;

describe("buildSegmentBody", () => {
  it("produces the exact Segment track shape", () => {
    const body = buildSegmentBody("cta_clicked", { surface: "hero" }, CTX);
    expect(body).toEqual({
      type: "track",
      event: "cta_clicked",
      anonymousId: "sid-123",
      properties: {
        surface: "hero",
        story: "homepage-hero",
        variant: "B",
        exp_key: "hero_copy",
      },
    });
  });
});

describe("track (server path, fetch mocked)", () => {
  const fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status: 200 })));

  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
    fetchMock.mockClear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    delete process.env.TELEMETRY_URL;
  });

  it("POSTs the Segment body to {TELEMETRY_URL}/v1/track with Basic dcl-sites auth", async () => {
    process.env.TELEMETRY_URL = "https://telemetry.example.com";

    trackAny("cta_clicked", { surface: "hero" }, CTX);
    await Promise.resolve();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];

    expect(url).toBe("https://telemetry.example.com/v1/track");
    expect(init.method).toBe("POST");
    const headers = init.headers as Record<string, string>;
    expect(headers.Authorization).toBe("Basic dcl-sites");
    expect(headers["Content-Type"]).toBe("application/json");

    expect(JSON.parse(init.body as string)).toEqual({
      type: "track",
      event: "cta_clicked",
      anonymousId: "sid-123",
      properties: {
        surface: "hero",
        story: "homepage-hero",
        variant: "B",
        exp_key: "hero_copy",
      },
    });
  });

  it("trims a trailing slash on TELEMETRY_URL", async () => {
    process.env.TELEMETRY_URL = "https://telemetry.example.com/";
    trackAny("evt", {}, CTX);
    await Promise.resolve();
    const [url] = fetchMock.mock.calls[0] as unknown as [string];
    expect(url).toBe("https://telemetry.example.com/v1/track");
  });

  it("does NOT throw and does NOT call fetch when TELEMETRY_URL is unset", async () => {
    expect(process.env.TELEMETRY_URL).toBeUndefined();
    expect(() => trackAny("evt", { a: 1 }, CTX)).not.toThrow();
    await Promise.resolve();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("never throws even when fetch itself throws", async () => {
    process.env.TELEMETRY_URL = "https://telemetry.example.com";
    fetchMock.mockImplementationOnce(() => {
      throw new Error("network down");
    });
    expect(() => trackAny("evt", {}, CTX)).not.toThrow();
    await Promise.resolve();
  });

  it("never throws with no env configured at all (full no-op)", () => {
    expect(() => trackAny("evt", { x: 1 }, { sid: "anon" })).not.toThrow();
  });
});

describe("trackExposure", () => {
  const fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status: 200 })));

  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
    fetchMock.mockClear();
    process.env.TELEMETRY_URL = "https://telemetry.example.com";
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    delete process.env.TELEMETRY_URL;
  });

  it("emits experiment_exposed with exp_key + variant", async () => {
    trackExposure(CTX);
    await Promise.resolve();

    const [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    const body = JSON.parse(init.body as string);
    expect(body.event).toBe(EXPERIMENT_EXPOSED);
    expect(body.properties.exp_key).toBe("hero_copy");
    expect(body.properties.variant).toBe("B");
  });
});
