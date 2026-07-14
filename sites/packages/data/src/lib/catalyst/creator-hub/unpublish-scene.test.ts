import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { generatePrivateKey } from "viem/accounts";

import { fetchParcelsPermission, unpublishWorldScene } from "./unpublish-scene";
import { createIdentityFromPrivateKey } from "../../auth/identity";
import type { AuthIdentity } from "../../auth/types";


const BASE = "https://worlds.example.test";

describe("unpublishWorldScene \u{2014} signed DELETE", () => {
  let identity: AuthIdentity;
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    identity = await createIdentityFromPrivateKey(generatePrivateKey());
    fetchMock = vi.fn(async () => new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => vi.unstubAllGlobals());

  it("DELETEs /world/{name}/scenes/{coord} signed with the ADR-44 chain", async () => {
    await unpublishWorldScene("my-world.dcl.eth", "0,0", { identity, base: BASE });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(init.method).toBe("DELETE");
    expect(url).toBe(`${BASE}/world/my-world.dcl.eth/scenes/0%2C0`);
    const headers = init.headers as Headers;
    expect(headers.get("x-identity-auth-chain-0")).toBeTruthy();
    expect(headers.get("x-identity-timestamp")).toBeTruthy();
  });

  it("throws (never claims success) on a 403 / 404", async () => {
    for (const status of [403, 404, 500]) {
      fetchMock.mockResolvedValueOnce(new Response("nope", { status }));
      await expect(
        unpublishWorldScene("w.dcl.eth", "1,1", { identity, base: BASE }),
      ).rejects.toThrow(/Unpublish failed/);
    }
  });
});

describe("fetchParcelsPermission", () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  beforeEach(() => {
    fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ total: 2, parcels: ["0,0", "0,1"] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => vi.unstubAllGlobals());

  it("GETs the deployment parcels path and parses the envelope", async () => {
    const res = await fetchParcelsPermission("my-world.dcl.eth", "0xABC");
    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url).toContain(
      "/world/my-world.dcl.eth/permissions/deployment/address/0xabc/parcels",
    );
    expect(res).toEqual({ total: 2, parcels: ["0,0", "0,1"] });
  });

  it("reports a failed read as null, not as an empty parcel list", async () => {
    // `{ parcels: [], total: 0 }` is a real answer meaning "this wallet holds
    // the permission world-wide", which world-settings reads as "may unpublish
    // every scene". Returning it on a 500 handed a collaborator the widest
    // reading of a permission nobody managed to look up.
    fetchMock.mockResolvedValueOnce(new Response("boom", { status: 500 }));
    expect(await fetchParcelsPermission("w.dcl.eth", "0x1")).toBeNull();
  });
});
