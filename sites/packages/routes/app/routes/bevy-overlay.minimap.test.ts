import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import * as track from "@core/lib/telemetry/track";
import { loader } from "./bevy-overlay.minimap";

function get(search = "") {
  return {
    request: new Request(`https://sites.test/bevy-overlay/minimap${search}`),
    params: {},
    context: {} as never,
  };
}

function pinsEnvelope(rows: unknown[]): Response {
  return new Response(
    JSON.stringify({ ok: true, data: rows, total: rows.length }),
    { status: 200, headers: { "content-type": "application/json" } },
  );
}

const GENESIS = { id: "plc-1", title: "Genesis Plaza", base_position: "-9,-9" };

async function dataFrom(search = "") {
  const res = await loader(get(search) as never);
  return res.data as { coords: string; place: string; heading: number | null };
}

beforeEach(() => {
  vi.restoreAllMocks();
  vi.spyOn(track, "trackExposure").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /bevy-overlay/minimap", () => {
  it("normalizes an explicit coords param", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("down"));
    expect((await dataFrom("?coords=%2012%20,%2034%20")).coords).toBe("12,34");
  });

  it("collapses garbage coords to 0,0", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("down"));
    expect((await dataFrom("?coords=junk")).coords).toBe("0,0");
  });

  it("falls back to the first map pin when no coords are given", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(pinsEnvelope([GENESIS]));
    const d = await dataFrom();
    expect(d.coords).toBe("-9,-9");
    expect(d.place).toBe("Genesis Plaza");
  });

  it("defaults to 0,0 with no place name when the map is unavailable", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("ECONNREFUSED"));
    const d = await dataFrom();
    expect(d.coords).toBe("0,0");
    expect(d.place).toBe("");
  });

  it("parses a finite heading and drops garbage", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("down"));
    expect((await dataFrom("?heading=12.5")).heading).toBe(12.5);
    expect((await dataFrom("?heading=north")).heading).toBeNull();
    expect((await dataFrom("?heading=")).heading).toBeNull();
    expect((await dataFrom()).heading).toBeNull();
  });
});
