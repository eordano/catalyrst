import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import * as track from "@core/lib/telemetry/track";
import { loader } from "./bevy-overlay.hud";

function get(search = "") {
  return {
    request: new Request(`https://sites.test/bevy-overlay/hud${search}`),
    params: {},
    context: {} as never,
  };
}

function json(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

async function dataFrom(search = "") {
  const res = await loader(get(search) as never);
  return res.data as {
    widget: string | null;
    realm: { configurations: { realmName: string | null } | null } | null;
  };
}

beforeEach(() => {
  vi.restoreAllMocks();
  vi.spyOn(track, "trackExposure").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /bevy-overlay/hud", () => {
  it("parses a known widget from the query", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("down"));
    expect((await dataFrom("?widget=profile")).widget).toBe("profile");
    expect((await dataFrom("?widget=connection")).widget).toBe("connection");
  });

  it("collapses an unknown widget to none", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("down"));
    expect((await dataFrom("?widget=bogus")).widget).toBeNull();
    expect((await dataFrom()).widget).toBeNull();
  });

  it("leaves realm null when /about is unreachable", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("ECONNREFUSED"));
    expect((await dataFrom()).realm).toBeNull();
  });

  it("leaves realm null on a malformed /about instead of throwing", async () => {
    for (const body of [null, "nope", []]) {
      vi.spyOn(globalThis, "fetch").mockResolvedValue(json(body));
      expect((await dataFrom()).realm).toBeNull();
    }
  });

  it("parses a healthy /about into the realm payload", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      json({ configurations: { realmName: "hela" } }),
    );
    expect((await dataFrom()).realm?.configurations?.realmName).toBe("hela");
  });
});
