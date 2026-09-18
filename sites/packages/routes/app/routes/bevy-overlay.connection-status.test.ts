import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import * as track from "@core/lib/telemetry/track";
import { resetRealmAboutCache } from "@data/lib/catalyst/realm-about.server";
import { loader } from "./bevy-overlay.connection-status";

type RealmStatus = {
  realmName: string | null;
  commsProtocol: string | null;
  usersCount: number | null;
  unavailable: boolean;
};

const NULL_REALM: RealmStatus = {
  realmName: null,
  commsProtocol: null,
  usersCount: null,
  unavailable: false,
};

function get() {
  return {
    request: new Request("https://sites.test/bevy-overlay/connection-status"),
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

async function realmFrom(): Promise<RealmStatus> {
  const res = await loader(get() as never);
  return (res.data as { realm: RealmStatus }).realm;
}

beforeEach(() => {
  vi.restoreAllMocks();
  resetRealmAboutCache();
  vi.spyOn(track, "trackExposure").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /bevy-overlay/connection-status", () => {
  it("degrades to the unavailable realm when /about is unreachable, errors or is not JSON", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("ECONNREFUSED"));
    expect(await realmFrom()).toEqual({ ...NULL_REALM, unavailable: true });
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("nope", { status: 503 }));
    expect(await realmFrom()).toEqual({ ...NULL_REALM, unavailable: true });
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("<html>down</html>", { status: 200 }));
    expect(await realmFrom()).toEqual({ ...NULL_REALM, unavailable: true });
  });

  it("never throws on a malformed /about shape", async () => {
    for (const body of [null, "nope", [], { configurations: 7, comms: "x" }]) {
      vi.spyOn(globalThis, "fetch").mockResolvedValue(json(body));
      expect(await realmFrom()).toEqual(NULL_REALM);
    }
  });

  it("reads realm name, comms protocol and user count from a healthy /about and nulls wrong types", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      json({
        healthy: true,
        configurations: { realmName: "hela" },
        comms: { protocol: "v3", usersCount: 42 },
      }),
    );
    expect(await realmFrom()).toEqual({
      realmName: "hela",
      commsProtocol: "v3",
      usersCount: 42,
      unavailable: false,
    });

    resetRealmAboutCache();
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      json({ configurations: { realmName: "hela" }, comms: { protocol: "", usersCount: "42" } }),
    );
    expect(await realmFrom()).toEqual({ ...NULL_REALM, realmName: "hela" });
  });
});
