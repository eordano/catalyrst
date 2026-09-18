import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { createIdentityFromPrivateKey } from "../../auth/identity";
import { signRequest } from "../../auth/signer";
import {
  action,
  loader,
} from "@routes/routes/api.creator-hub.drafts.$";
import {
  DraftStoreUnavailable,
  draftsRoot,
  getDraft,
  listDrafts,
  putDraft,
  verifyAuthChainRequest,
} from "./scene-drafts.server";

const A = "0x7bbea9c18cd0541acab8c19da2b11d0c03faef1c";
const B = "0x1111111111111111111111111111111111111111";

let dir: string;

beforeEach(async () => {
  dir = await mkdtemp(path.join(tmpdir(), "ch-drafts-"));
  process.env.CH_DRAFTS_DIR = dir;
});

afterEach(async () => {
  delete process.env.CH_DRAFTS_DIR;
  await rm(dir, { recursive: true, force: true });
});

async function signed(
  key: `0x${string}`,
  method: string,
  pathname: string,
  body?: unknown,
  identityOpts: { expirationMs?: number } = {},
): Promise<{ request: Request; wallet: string }> {
  const identity = await createIdentityFromPrivateKey(key, identityOpts);
  const url = `http://localhost${pathname}`;
  const s = await signRequest(identity, method, pathname, {});
  const headers = new Headers(s.headers);
  if (body !== undefined) headers.set("content-type", "application/json");
  return {
    request: new Request(url, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    }),
    wallet: identity.signer,
  };
}

async function signedReq(
  key: `0x${string}`,
  method: string,
  pathname: string,
  body?: unknown,
): Promise<Request> {
  return (await signed(key, method, pathname, body)).request;
}

describe("scene-drafts store: durable persistence + optimistic concurrency", () => {
  it("creates at version 1, round-trips the blob byte-equal, and re-uses the stored title when a put omits it", async () => {
    const blob = {
      composite: { version: 1, components: [{ name: "Transform" }] },
      files: { "main.crdt": "AQID", "assets/cat.glb": "bafkreicat" },
      nested: { arr: [true, null, "x", 3.14], deep: { k: "v" } },
    };
    const created = await putDraft(A, "-86,78", {
      baseVersion: 0,
      hash: "hash-1",
      blob,
      title: "UAP",
    });
    expect(created.ok).toBe(true);
    if (!created.ok) return;
    expect(created.meta.version).toBe(1);
    expect(created.meta.title).toBe("UAP");
    expect(created.meta.hash).toBe("hash-1");
    expect(created.meta.updatedAt).toBeGreaterThan(0);

    const pulled = await getDraft(A, "-86,78");
    expect(pulled).not.toBeNull();
    expect(pulled!.version).toBe(1);
    expect(pulled!.blob).toEqual(blob);
    expect(JSON.stringify(pulled!.blob)).toBe(JSON.stringify(blob));

    const next = await putDraft(A, "-86,78", { baseVersion: 1, hash: "hash-2", blob: { x: 1 } });
    expect(next.ok).toBe(true);
    if (!next.ok) return;
    expect(next.meta.title).toBe("UAP");
  });

  it("matching baseVersion advances; stale baseVersion conflicts with the server copy and does not write", async () => {
    const c = await putDraft(A, "scene", { baseVersion: 0, hash: "h1", blob: { v: 1 } });
    expect(c.ok).toBe(true);

    const advanced = await putDraft(A, "scene", {
      baseVersion: 1,
      hash: "h2",
      blob: { v: 2 },
      title: "second",
    });
    expect(advanced.ok).toBe(true);
    if (!advanced.ok) return;
    expect(advanced.meta.version).toBe(2);

    const stale = await putDraft(A, "scene", { baseVersion: 1, hash: "h3", blob: { v: 3 } });
    expect(stale.ok).toBe(false);
    if (stale.ok) return;
    expect(stale.conflict).toBe(true);
    expect(stale.server.version).toBe(2);
    expect(stale.server.blob).toEqual({ v: 2 });
    expect(stale.server.title).toBe("second");

    const current = await getDraft(A, "scene");
    expect(current!.version).toBe(2);
    expect(current!.blob).toEqual({ v: 2 });
  });
});

describe("scene-drafts store: location + unavailable store", () => {
  it("resolves the root from CH_DRAFTS_DIR, then SITES_STATE_DIR, then cwd; an unwritable root is DraftStoreUnavailable and the route answers 503 with retry-after", async () => {
    expect(draftsRoot()).toBe(dir);
    delete process.env.CH_DRAFTS_DIR;
    process.env.SITES_STATE_DIR = "/var/lib/sites-state";
    expect(draftsRoot()).toBe(path.join("/var/lib/sites-state", "ch-drafts"));
    delete process.env.SITES_STATE_DIR;
    expect(draftsRoot()).toBe(path.join(process.cwd(), "data", "ch-drafts"));

    const blocker = path.join(dir, "blocker");
    await writeFile(blocker, "not a directory", "utf8");
    process.env.CH_DRAFTS_DIR = path.join(blocker, "ch-drafts");
    await expect(
      putDraft(A, "scene", { baseVersion: 0, hash: "h", blob: { v: 1 } }),
    ).rejects.toBeInstanceOf(DraftStoreUnavailable);
    await expect(listDrafts(A)).rejects.toBeInstanceOf(DraftStoreUnavailable);
    await expect(getDraft(A, "scene")).rejects.toBeInstanceOf(DraftStoreUnavailable);

    const key = generatePrivateKey();
    const idPath = "/creator-hub/drafts/untitled-scene";
    const res = await action({
      request: await signedReq(key, "PUT", idPath, { baseVersion: 0, hash: "h1", blob: { v: 1 } }),
      params: { "*": "untitled-scene" },
    });
    expect(res.status).toBe(503);
    expect(Number(res.headers.get("retry-after"))).toBeGreaterThan(0);
    const body = (await res.json()) as { error: string };
    expect(body.error).toContain("CH_DRAFTS_DIR");
  });
});

describe("scene-drafts store: wallet scoping + path safety", () => {
  it("lists only the requesting wallet's drafts newest first, and wallet A cannot read wallet B's draft", async () => {
    await putDraft(A, "a", { baseVersion: 0, hash: "h", blob: {} });
    await new Promise((r) => setTimeout(r, 2));
    await putDraft(A, "b", { baseVersion: 0, hash: "h", blob: {} });
    await putDraft(B, "c", { baseVersion: 0, hash: "h", blob: {} });

    const listA = await listDrafts(A);
    expect(listA.map((d) => d.id).sort()).toEqual(["a", "b"]);
    expect(listA[0].id).toBe("b");

    const listB = await listDrafts(B);
    expect(listB.map((d) => d.id)).toEqual(["c"]);

    await putDraft(B, "secret", { baseVersion: 0, hash: "h", blob: { s: 42 } });
    expect(await getDraft(A, "secret")).toBeNull();
    expect(await getDraft(B, "secret")).not.toBeNull();
  });

  it("rejects path-traversal / unsafe ids, never escapes the wallet dir, and rejects an invalid wallet", async () => {
    await expect(
      putDraft(A, "../evil", { baseVersion: 0, hash: "h", blob: {} }),
    ).rejects.toThrow();
    await expect(
      putDraft(A, "..", { baseVersion: 0, hash: "h", blob: {} }),
    ).rejects.toThrow();
    await expect(
      putDraft(A, "a/b", { baseVersion: 0, hash: "h", blob: {} }),
    ).rejects.toThrow();
    await expect(
      putDraft(A, ".", { baseVersion: 0, hash: "h", blob: {} }),
    ).rejects.toThrow();
    expect(await getDraft(A, "../evil")).toBeNull();

    await expect(
      putDraft("not-a-wallet", "x", { baseVersion: 0, hash: "h", blob: {} }),
    ).rejects.toThrow();
    expect(await listDrafts("0xshort")).toEqual([]);
    expect(await getDraft("0xshort", "x")).toBeNull();
  });
});

describe("scene-drafts auth: ADR-44 AuthChain verification", () => {
  it("verifies a real signed request, derives the wallet from the chain, and rejects an unauthenticated request with 401 at the verifier and the route", async () => {
    const key = generatePrivateKey();
    const { request, wallet } = await signed(key, "GET", "/creator-hub/drafts");
    const res = await verifyAuthChainRequest(request);
    expect(res.ok).toBe(true);
    if (!res.ok) return;
    expect(res.wallet).toBe(wallet);
    expect(res.wallet).toBe(privateKeyToAccount(key).address.toLowerCase());

    const anon = await verifyAuthChainRequest(
      new Request("http://localhost/creator-hub/drafts"),
    );
    expect(anon.ok).toBe(false);
    if (anon.ok) return;
    expect(anon.status).toBe(401);

    const list = await loader({
      request: new Request("http://localhost/creator-hub/drafts"),
      params: { "*": "" },
    });
    expect(list.status).toBe(401);
  });

  it("rejects an expired ephemeral identity, a stale timestamp, and a signature replayed across paths with 401", async () => {
    const expiredKey = generatePrivateKey();
    const expired = await signed(expiredKey, "GET", "/creator-hub/drafts", undefined, {
      expirationMs: 1_000,
    });
    const expiredTs = Number(expired.request.headers.get("x-identity-timestamp"));
    const expiredRes = await verifyAuthChainRequest(expired.request, { now: expiredTs + 2_000 });
    expect(expiredRes.ok).toBe(false);
    if (expiredRes.ok) return;
    expect(expiredRes.status).toBe(401);
    expect(expiredRes.error).toContain("expired");

    const staleKey = generatePrivateKey();
    const stale = await signed(staleKey, "GET", "/creator-hub/drafts");
    const staleTs = Number(stale.request.headers.get("x-identity-timestamp"));
    const staleRes = await verifyAuthChainRequest(stale.request, { now: staleTs + 20 * 60_000 });
    expect(staleRes.ok).toBe(false);
    if (staleRes.ok) return;
    expect(staleRes.status).toBe(401);

    const replayKey = generatePrivateKey();
    const { request } = await signed(replayKey, "GET", "/creator-hub/drafts/one");
    const moved = new Request("http://localhost/creator-hub/drafts/two", {
      method: "GET",
      headers: request.headers,
    });
    const replayRes = await verifyAuthChainRequest(moved);
    expect(replayRes.ok).toBe(false);
    if (replayRes.ok) return;
    expect(replayRes.status).toBe(401);
  });
});

describe("scene-drafts route: authenticated PUT + GET round trip", () => {
  it("GET of a missing draft is 404; PUT persists under the verified wallet (ignoring any body-supplied address) at version 1; GET reads it back", async () => {
    const key = generatePrivateKey();
    const wallet = privateKeyToAccount(key).address.toLowerCase();
    const idPath = "/creator-hub/drafts/-86,78";

    const missing = await loader({
      request: await signedReq(key, "GET", idPath),
      params: { "*": "-86,78" },
    });
    expect(missing.status).toBe(404);
    const signedOptional=await signedReq(key,"GET",idPath);
    const optional=await loader({request:new Request(signedOptional.url+"?optional=1",{headers:signedOptional.headers}),params:{"*":"-86,78"}});
    expect(optional.status).toBe(204);
    expect(await optional.text()).toBe("");
    expect(optional.headers.get("cache-control")).toBe("private, no-store");

    const putReq = await signedReq(key, "PUT", idPath, {
      baseVersion: 0,
      hash: "hh",
      blob: { composite: "yes" },
      title: "UAP",
      wallet: B,
    });
    const putRes = await action({ request: putReq, params: { "*": "-86,78" } });
    expect(putRes.status).toBe(200);
    const putBody = (await putRes.json()) as PutResultBody;
    expect(putBody.ok).toBe(true);
    if (!putBody.ok) return;
    expect(putBody.meta.version).toBe(1);

    expect(await getDraft(wallet, "-86,78")).not.toBeNull();
    expect(await getDraft(B, "-86,78")).toBeNull();

    const getReq = await signedReq(key, "GET", idPath);
    const getRes = await loader({ request: getReq, params: { "*": "-86,78" } });
    expect(getRes.status).toBe(200);
    const draft = (await getRes.json()) as { version: number; blob: unknown };
    expect(draft.version).toBe(1);
    expect(draft.blob).toEqual({ composite: "yes" });
  });

  it("PUT with a stale baseVersion returns 409 with the server copy", async () => {
    const key = generatePrivateKey();
    const idPath = "/creator-hub/drafts/s";
    await action({
      request: await signedReq(key, "PUT", idPath, { baseVersion: 0, hash: "h1", blob: { v: 1 } }),
      params: { "*": "s" },
    });
    await action({
      request: await signedReq(key, "PUT", idPath, { baseVersion: 1, hash: "h2", blob: { v: 2 } }),
      params: { "*": "s" },
    });
    const conflictRes = await action({
      request: await signedReq(key, "PUT", idPath, { baseVersion: 1, hash: "h3", blob: { v: 3 } }),
      params: { "*": "s" },
    });
    expect(conflictRes.status).toBe(409);
    const body = (await conflictRes.json()) as PutResultBody;
    expect(body.ok).toBe(false);
    if (body.ok) return;
    expect(body.conflict).toBe(true);
    expect(body.server.version).toBe(2);
  });
});

type PutResultBody =
  | { ok: true; meta: { version: number } }
  | { ok: false; conflict: true; server: { version: number } };
