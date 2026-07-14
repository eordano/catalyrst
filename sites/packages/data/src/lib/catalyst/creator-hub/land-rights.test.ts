import { afterEach, describe, expect, it, vi } from "vitest";

import {
  DEPLOY_GRANTING_LEGS,
  fetchParcelOwner,
  fetchParcelPermissions,
  grantsDeploy,
  parseParcel,
  probeLandRights,
} from "./land-rights";

const WALLET = "0x1111111111111111111111111111111111111111";
const OWNER = "0x2222222222222222222222222222222222222222";

const NO_RIGHTS = {
  owner: false,
  operator: false,
  updateOperator: false,
  updateManager: false,
  approvedForAll: false,
};

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function mockFetch(handler: (url: string) => Response | Promise<Response>) {
  const fn = vi.fn(async (input: RequestInfo | URL) => handler(String(input)));
  vi.stubGlobal("fetch", fn);
  return fn;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("parseParcel", () => {
  it("parses signed coordinates and rejects non-parcels", () => {
    expect(parseParcel("52,-52")).toEqual({ x: 52, y: -52 });
    expect(parseParcel(" -1 , 8 ")).toEqual({ x: -1, y: 8 });
    expect(parseParcel("myworld.dcl.eth")).toBeNull();
    expect(parseParcel("52")).toBeNull();
    expect(parseParcel("")).toBeNull();
  });
});

describe("grantsDeploy", () => {
  it("denies with no legs and grants on each server-exported granting leg", () => {
    expect(grantsDeploy(NO_RIGHTS)).toBe(false);
    for (const leg of DEPLOY_GRANTING_LEGS) {
      expect(grantsDeploy({ ...NO_RIGHTS, [leg]: true })).toBe(true);
    }
  });

  it("mirrors the validator's five legs", () => {
    expect([...DEPLOY_GRANTING_LEGS].sort()).toEqual(
      Object.keys(NO_RIGHTS).sort(),
    );
  });
});

describe("fetchParcelPermissions", () => {
  it("hits the lambdas permissions route with lowercased address", async () => {
    const fn = mockFetch(() => jsonResponse({ ...NO_RIGHTS, operator: true }));
    const flags = await fetchParcelPermissions(WALLET.toUpperCase(), "52,-52");
    expect(flags).toEqual({ ...NO_RIGHTS, operator: true });
    expect(fn.mock.calls[0][0]).toContain(
      `/lambdas/users/${WALLET}/parcels/52/-52/permissions`,
    );
  });

  it("treats a 404 (no parcel/estate) as no rights", async () => {
    mockFetch(() => jsonResponse({ error: "not found" }, 404));
    const flags = await fetchParcelPermissions(WALLET, "52,-52");
    expect(flags).toEqual(NO_RIGHTS);
  });

  it("throws on a 500 so callers can fail closed", async () => {
    mockFetch(() => jsonResponse({}, 500));
    await expect(fetchParcelPermissions(WALLET, "52,-52")).rejects.toThrow();
  });
});

describe("fetchParcelOwner", () => {
  it("reads owner from the operators route and swallows failures", async () => {
    const fn = mockFetch(() => jsonResponse({ owner: OWNER, operator: null }));
    expect(await fetchParcelOwner("52,-52")).toBe(OWNER);
    expect(fn.mock.calls[0][0]).toContain("/lambdas/parcels/52/-52/operators");

    mockFetch(() => jsonResponse({}, 500));
    expect(await fetchParcelOwner("52,-52")).toBeNull();
  });
});

describe("probeLandRights", () => {
  it("grants only when every parcel grants", async () => {
    mockFetch(() => jsonResponse({ ...NO_RIGHTS, owner: true }));
    const rights = await probeLandRights(WALLET, ["52,-52", "53,-52", "52,-52"]);
    expect(rights).toEqual({ status: "granted", parcels: ["52,-52", "53,-52"] });
  });

  it("denies with the owning address when one parcel has no rights", async () => {
    mockFetch((url) => {
      if (url.includes("/operators")) return jsonResponse({ owner: OWNER });
      if (url.includes("/53/")) return jsonResponse(NO_RIGHTS);
      return jsonResponse({ ...NO_RIGHTS, owner: true });
    });
    const rights = await probeLandRights(WALLET, ["52,-52", "53,-52"]);
    expect(rights).toEqual({ status: "denied", parcel: "53,-52", owner: OWNER });
  });

  it("fails closed to unknown on probe errors", async () => {
    mockFetch(() => jsonResponse({}, 500));
    const rights = await probeLandRights(WALLET, ["52,-52"]);
    expect(rights).toEqual({ status: "unknown", parcel: "52,-52" });
  });

  it("is unknown for world pointers and empty input", async () => {
    mockFetch(() => jsonResponse(NO_RIGHTS));
    expect(await probeLandRights(WALLET, ["myworld.dcl.eth"])).toEqual({
      status: "unknown",
      parcel: "myworld.dcl.eth",
    });
    expect(await probeLandRights(WALLET, [])).toEqual({ status: "unknown", parcel: "" });
    expect(await probeLandRights("", ["52,-52"])).toEqual({
      status: "unknown",
      parcel: "52,-52",
    });
  });
});
