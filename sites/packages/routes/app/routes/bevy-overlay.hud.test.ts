import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import * as track from "@core/lib/telemetry/track";
import { resetRealmAboutCache } from "@data/lib/catalyst/realm-about.server";
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
  resetRealmAboutCache();
  vi.spyOn(track, "trackExposure").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /bevy-overlay/hud", () => {
  it("parses a known widget from the query and collapses unknown or missing ones to none", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("down"));
    expect((await dataFrom("?widget=profile")).widget).toBe("profile");
    expect((await dataFrom("?widget=connection")).widget).toBe("connection");
    expect((await dataFrom("?widget=bogus")).widget).toBeNull();
    expect((await dataFrom()).widget).toBeNull();
  });

  it("parses a healthy /about into the realm payload and leaves realm null when it is unreachable or malformed", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(json({ configurations: { realmName: "hela" } }));
    expect((await dataFrom()).realm?.configurations?.realmName).toBe("hela");

    resetRealmAboutCache();
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("ECONNREFUSED"));
    expect((await dataFrom()).realm).toBeNull();
    for (const body of [null, "nope", []]) {
      vi.spyOn(globalThis, "fetch").mockResolvedValue(json(body));
      expect((await dataFrom()).realm).toBeNull();
    }
  });

  it("serves a healthy /about from the server memo on the next request", async () => {
    const spy = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(async () => json({ configurations: { realmName: "hela" } }));
    await dataFrom();
    const aboutCalls = () => spy.mock.calls.filter(([u]) => String(u).endsWith("/about")).length;
    expect(aboutCalls()).toBe(1);
    expect((await dataFrom()).realm?.configurations?.realmName).toBe("hela");
    expect(aboutCalls()).toBe(1);
  });
});
