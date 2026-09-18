import { afterEach, describe, expect, it, vi } from "vitest";

import {
  asAddress,
  deployerFromAuthChain,
  fetchSceneDeployment,
  fetchLandOwner,
  fetchWorldSceneDeployment,
  sceneRecipients,
  fetchWorldOwner,
  worldRealmBase,
  homeRealmFromAbout,
  isHomeRealm,
  sceneEntityForParcel,
} from "./sceneOwner";

const DEPLOYER = "0xD6EFF8F07CAF3443A1178407D3DE4129149D6EF6";

it("resolves a matching world's owner, never a deployer allow-list or a stale realm", async () => {
  const base = "https://catalyst.test/world/tophub.dcl.eth";
  expect(worldRealmBase("tophub", `${base}/about`)).toBe(base);
  expect(worldRealmBase("tophub", "https://catalyst.test")).toBeNull();
  const fetchImpl = vi.fn(async (url: string | URL | Request) => new Response(JSON.stringify(
    String(url).endsWith("/about") ? { configurations: { realmName: "tophub" } } : { owner: DEPLOYER, permissions: { deployment: { wallets: ["someone else"] } } },
  )));
  expect(await fetchWorldOwner("tophub", base, { fetchImpl })).toEqual({ deployer: DEPLOYER.toLowerCase(), title: "tophub" });
  expect(await fetchWorldOwner("previous-world", base, { fetchImpl })).toBeNull();
  expect(fetchImpl.mock.calls.filter(([url]) => String(url).endsWith("/permissions"))).toHaveLength(1);
});

const AUDIT = {
  version: "v3",
  localTimestamp: 1786397392145,
  authChain: [
    { type: "SIGNER", payload: DEPLOYER },
    { type: "ECDSA_EPHEMERAL", payload: "Decentraland Login", signature: "0x1" },
    { type: "ECDSA_SIGNED_ENTITY", payload: "bafkrei", signature: "0x2" },
  ],
};

const ENTITIES = [
  {
    id: "bafkreibg66k5",
    type: "scene",
    pointers: ["-147,91", "-143,102", "-146,91"],
    metadata: { display: { title: "CBD Plaza" }, scene: { base: "-147,91" } },
  },
];

describe("asAddress", () => {
  it("lowercases checksummed or padded addresses and rejects names, short hex and non-strings", () => {
    expect(asAddress(DEPLOYER)).toBe(DEPLOYER.toLowerCase());
    expect(asAddress(` ${DEPLOYER.toLowerCase()} `)).toBe(DEPLOYER.toLowerCase());
    expect(asAddress("dhingia builds")).toBeNull();
    expect(asAddress("0x1234")).toBeNull();
    expect(asAddress(null)).toBeNull();
    expect(asAddress(42)).toBeNull();
  });
});

describe("sceneEntityForParcel", () => {
  it("finds the entity whose pointers cover the parcel, with a null title when there is none, else null", () => {
    expect(sceneEntityForParcel(ENTITIES, "-143,102")).toEqual({
      id: "bafkreibg66k5",
      title: "CBD Plaza",
    });
    expect(sceneEntityForParcel([{ id: "x", pointers: ["1,1"], metadata: {} }], "1,1")).toEqual({
      id: "x",
      title: null,
    });
    expect(sceneEntityForParcel(ENTITIES, "0,0")).toBeNull();
    expect(sceneEntityForParcel([], "-143,102")).toBeNull();
    expect(sceneEntityForParcel({ data: ENTITIES }, "-143,102")).toBeNull();
    expect(sceneEntityForParcel([{ pointers: ["-143,102"] }], "-143,102")).toBeNull();
  });
});

describe("deployerFromAuthChain", () => {
  it("takes the SIGNER link of the audit auth chain or null without a valid signer", () => {
    expect(deployerFromAuthChain(AUDIT)).toBe(DEPLOYER.toLowerCase());
    expect(deployerFromAuthChain({ authChain: [] })).toBeNull();
    expect(deployerFromAuthChain({ authChain: [{ type: "SIGNER", payload: "nope" }] })).toBeNull();
    expect(deployerFromAuthChain(null)).toBeNull();
  });
});

describe("fetchSceneDeployment", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("resolves the parcel to its active entity then the deployer from the audit, and skips the audit when there is no scene", async () => {
    const calls: { url: string; method: string; body: string | undefined }[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        calls.push({ url, method: init?.method ?? "GET", body: init?.body as string | undefined });
        const payload = url.endsWith("/content/entities/active") ? ENTITIES : AUDIT;
        return new Response(JSON.stringify(payload), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }),
    );
    const out = await fetchSceneDeployment("-143,102", { base: "https://dcl.test" });
    expect(out).toEqual({
      entityId: "bafkreibg66k5",
      title: "CBD Plaza",
      deployer: DEPLOYER.toLowerCase(),
    });
    expect(calls.map((c) => `${c.method} ${c.url}`)).toEqual([
      "POST https://dcl.test/content/entities/active",
      "GET https://dcl.test/content/audit/scene/bafkreibg66k5",
    ]);
    expect(JSON.parse(calls[0]!.body!)).toEqual({ pointers: ["-143,102"] });

    const fetchMock = vi.fn(async () => new Response("[]", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    expect(await fetchSceneDeployment("0,0", { base: "https://dcl.test" })).toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});

describe("home realm detection", () => {
  const home = homeRealmFromAbout(
    { configurations: { realmName: "dcl-one", networkId: 1 } },
    "https://catalyst.example.com",
  );

  it("treats only the server's own realm name or about URL as land, never worlds, foreign realms or unknowns", () => {
    expect(home).toEqual({ name: "dcl-one", base: "https://catalyst.example.com" });
    expect(homeRealmFromAbout({}, "https://catalyst.example.com")).toEqual({ name: null, base: "https://catalyst.example.com" });
    for (const realm of ["dcl-one", " DCL-One ", "https://catalyst.example.com/about", "https://catalyst.example.com/"]) {
      expect(isHomeRealm(realm, home), realm).toBe(true);
    }
    for (const realm of [
      "foo",
      "foo.dcl.eth",
      "https://catalyst.example.com/world/foo/about",
      "https://peer.decentraland.org/about",
      "main",
      null,
      "",
    ]) {
      expect(isHomeRealm(realm, home), String(realm)).toBe(false);
    }
    expect(isHomeRealm("dcl-one", null)).toBe(false);
    expect(isHomeRealm("dcl-one", { name: null, base: "" })).toBe(false);
  });
});


it("recognizes the public content origin when the player uses the site proxy", () => {
  const home = homeRealmFromAbout({ configurations: { realmName: "dcl-one" }, content: { publicUrl: "https://catalyst.example.com/content" } }, "https://catalyst.example.com");
  expect(isHomeRealm("https://catalyst.example.com", home)).toBe(true);
  expect(isHomeRealm("https://catalyst.example.com/about", home)).toBe(true);
  expect(isHomeRealm("https://another.example", home)).toBe(false);
});

const TIP = "0x1111111111111111111111111111111111111111";
const AUTHOR = "0x2222222222222222222222222222222222222222";
const LAND = "0x3333333333333333333333333333333333333333";
const RECIPIENT_ENTITY = { ...ENTITIES[0], metadata: { tipAddress: TIP, sceneAuthor: { address: AUTHOR }, sceneDeployer: TIP, landOwner: TIP } };

it("keeps scene metadata, deployment signer and blockchain owner separate", async () => {
  const fetchImpl = vi.fn(async (url: string | URL | Request) => new Response(JSON.stringify(
    String(url).endsWith("/operators") ? { owner: LAND, operator: TIP, updateOperator: TIP }
      : String(url).endsWith("/entities/active") ? [RECIPIENT_ENTITY] : AUDIT,
  )));
  const opts = { base: "https://catalyst.test", fetchImpl };
  const deployment = await fetchSceneDeployment("-143,102", opts);
  const owner = await fetchLandOwner("-143,102", opts);
  expect(sceneRecipients(deployment, owner).map(row => [row.role, row.address])).toEqual([
    ["tipAddress", TIP], ["sceneAuthor", AUTHOR], ["sceneDeployer", DEPLOYER.toLowerCase()], ["landOwner", LAND],
  ]);
  expect(fetchImpl.mock.calls.at(-1)?.[0]).toBe("https://catalyst.test/lambdas/parcels/-143/102/operators");
});

it("retains scene-provided addresses when the deployment audit is unavailable", async () => {
  const fetchImpl = vi.fn(async (url: string | URL | Request) => String(url).endsWith("/entities/active")
    ? new Response(JSON.stringify([RECIPIENT_ENTITY])) : new Response("unavailable", { status: 503 }));
  expect(await fetchSceneDeployment("-143,102", { fetchImpl })).toMatchObject({ tipAddress: TIP, sceneAuthor: AUTHOR, deployer: null });
});

it("reads a World's actual scene and deployment signer, without mistaking permissions for LAND ownership", async () => {
  const base = "https://catalyst.test/world/test.eth";
  const fetchImpl = vi.fn(async (url: string | URL | Request) => new Response(JSON.stringify(
    String(url).endsWith("/about") ? { configurations: { realmName: "test.eth", scenesUrn: ["urn:decentraland:entity:bafkreibg66k5"] }, content: { publicUrl: "https://catalyst.test/content" } }
      : String(url).includes("/audit/") ? AUDIT : RECIPIENT_ENTITY,
  )));
  const deployment = await fetchWorldSceneDeployment("test.eth", base, "-143,102", { fetchImpl });
  expect(deployment).toMatchObject({ sceneAuthor: AUTHOR, deployer: DEPLOYER.toLowerCase() });
  expect(sceneRecipients(deployment, LAND, true).at(-1)).toMatchObject({ address: null, unavailable: "Not applicable in Worlds" });
  expect(fetchImpl.mock.calls.some(([url]) => String(url).includes("/permissions"))).toBe(false);
});
