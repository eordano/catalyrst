import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { generatePrivateKey } from "viem/accounts";

import {
  clearEnvKeys,
  clearValues,
  deleteEnvKey,
  deleteValue,
  saveEnvKey,
  saveValue,
  type WriteOptions,
} from "./worlds-storage";
import { createIdentityFromPrivateKey } from "../../auth/identity";
import type { AuthIdentity } from "../../auth/types";

const BASE = "https://worlds.example.test";
const SCOPE = { realm: "my-world.dcl.eth", parcel: "1,2" } as const;

describe("worlds-storage signed writes", () => {
  let identity: AuthIdentity;
  let fetchMock: ReturnType<typeof vi.fn>;
  let opts: WriteOptions;

  beforeEach(async () => {
    identity = await createIdentityFromPrivateKey(generatePrivateKey());
    fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    opts = { identity, base: BASE, scope: { ...SCOPE } };
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function captured() {
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    fetchMock.mockClear();
    return { url, init, headers: init.headers as Headers };
  }

  function link(headers: Headers, i: number) {
    const raw = headers.get(`x-identity-auth-chain-${i}`);
    expect(raw, `auth-chain link ${i}`).toBeTruthy();
    return JSON.parse(raw as string) as {
      type: string;
      payload: string;
      signature: string;
    };
  }

  function expectSignedAs(
    headers: Headers,
    method: string,
    path: string,
  ) {
    expect(link(headers, 0).type).toBe("SIGNER");
    expect(link(headers, 0).payload).toBe(identity.signer);
    expect(link(headers, 1).type).toBe("ECDSA_EPHEMERAL");
    expect(link(headers, 1).payload).toBe(identity.authChain[1].payload);

    const entity = link(headers, 2);
    expect(entity.type).toBe("ECDSA_SIGNED_ENTITY");
    expect(entity.signature).toMatch(/^0x[0-9a-f]+$/i);

    const ts = headers.get("x-identity-timestamp");
    const meta = headers.get("x-identity-metadata");
    expect(ts).toMatch(/^\d+$/);
    expect(meta).toBeTruthy();
    expect(entity.payload).toBe(`${method}:${path}:${ts}:${meta}`.toLowerCase());
  }

  it("saveValue: PUT /world-storage/values/{key} with the value verbatim as the {value} JSON body", async () => {
    await saveValue("highScore", 42, opts);
    const first = captured();
    expect(first.url).toBe(`${BASE}/world-storage/values/highScore`);
    expect(first.init.method).toBe("PUT");
    expect(first.headers.get("content-type")).toBe("application/json");
    expect(first.init.body).toBe(JSON.stringify({ value: 42 }));
    expectSignedAs(first.headers, "PUT", "/world-storage/values/highScore");

    await saveValue("puzzle.state", { level: 4, done: false }, opts);
    expect(captured().init.body).toBe(JSON.stringify({ value: { level: 4, done: false } }));
  });

  it("env keys: PUT /world-storage/env/{key} with a string value, DELETE without a body, and clear with the confirm header", async () => {
    await saveEnvKey("API_URL", "https://api.example", opts);
    const put = captured();
    expect(put.url).toBe(`${BASE}/world-storage/env/API_URL`);
    expect(put.init.method).toBe("PUT");
    expect(put.init.body).toBe(JSON.stringify({ value: "https://api.example" }));
    expectSignedAs(put.headers, "PUT", "/world-storage/env/API_URL");

    await deleteEnvKey("API_URL", opts);
    const del = captured();
    expect(del.url).toBe(`${BASE}/world-storage/env/API_URL`);
    expect(del.init.method).toBe("DELETE");
    expect(del.init.body).toBeUndefined();
    expectSignedAs(del.headers, "DELETE", "/world-storage/env/API_URL");

    await clearEnvKeys(opts);
    const clear = captured();
    expect(clear.url).toBe(`${BASE}/world-storage/env`);
    expect(clear.init.method).toBe("DELETE");
    expect(clear.headers.get("x-confirm-delete-all")).toBe("true");
    expectSignedAs(clear.headers, "DELETE", "/world-storage/env");
  });

  it("values: DELETE /world-storage/values/{key} without a body or confirm header, and clear only with the confirm header", async () => {
    await deleteValue("highScore", opts);
    const del = captured();
    expect(del.url).toBe(`${BASE}/world-storage/values/highScore`);
    expect(del.init.method).toBe("DELETE");
    expect(del.init.body).toBeUndefined();
    expect(del.headers.get("content-type")).toBeNull();
    expect(del.headers.get("x-confirm-delete-all")).toBeNull();
    expectSignedAs(del.headers, "DELETE", "/world-storage/values/highScore");

    await clearValues(opts);
    const clear = captured();
    expect(clear.url).toBe(`${BASE}/world-storage/values`);
    expect(clear.init.method).toBe("DELETE");
    expect(clear.init.body).toBeUndefined();
    expect(clear.headers.get("x-confirm-delete-all")).toBe("true");
    expectSignedAs(clear.headers, "DELETE", "/world-storage/values");
  });

  it("folds the realm + parcel into the signed metadata, URL-encodes keys and signs the encoded path, and forwards the abort signal", async () => {
    await deleteValue("k", opts);
    const meta = JSON.parse(captured().headers.get("x-identity-metadata") as string);
    expect(meta).toEqual({
      parcel: "1,2",
      realm: { serverName: "my-world.dcl.eth" },
      realmName: "my-world.dcl.eth",
    });

    await saveValue("a/b c", 1, opts);
    const encoded = captured();
    expect(encoded.url).toBe(`${BASE}/world-storage/values/a%2Fb%20c`);
    expectSignedAs(encoded.headers, "PUT", "/world-storage/values/a%2Fb%20c");

    const controller = new AbortController();
    await saveValue("k", 1, { ...opts, signal: controller.signal });
    expect(captured().init.signal).toBe(controller.signal);
  });

  it("throws (never resolves) when the server rejects the write or the network fetch fails", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(null, { status: 403, statusText: "Forbidden" }),
    );
    await expect(deleteValue("k", opts)).rejects.toThrow(/403/);

    fetchMock.mockRejectedValueOnce(new Error("boom"));
    await expect(saveValue("k", 1, opts)).rejects.toThrow(/boom|failed/i);
  });
});
