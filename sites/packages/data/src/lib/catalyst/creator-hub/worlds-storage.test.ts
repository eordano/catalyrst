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

  it("saveValue: PUT /world-storage/values/{key} with {value} body", async () => {
    await saveValue("highScore", 42, opts);

    const { url, init, headers } = captured();
    expect(url).toBe(`${BASE}/world-storage/values/highScore`);
    expect(init.method).toBe("PUT");
    expect(headers.get("content-type")).toBe("application/json");
    expect(init.body).toBe(JSON.stringify({ value: 42 }));
    expectSignedAs(headers, "PUT", "/world-storage/values/highScore");
  });

  it("saveValue: sends the value verbatim as JSON (objects preserved)", async () => {
    await saveValue("puzzle.state", { level: 4, done: false }, opts);
    const { init } = captured();
    expect(init.body).toBe(JSON.stringify({ value: { level: 4, done: false } }));
  });

  it("saveEnvKey: PUT /world-storage/env/{key} with a string value", async () => {
    await saveEnvKey("API_URL", "https://api.example", opts);

    const { url, init, headers } = captured();
    expect(url).toBe(`${BASE}/world-storage/env/API_URL`);
    expect(init.method).toBe("PUT");
    expect(init.body).toBe(JSON.stringify({ value: "https://api.example" }));
    expectSignedAs(headers, "PUT", "/world-storage/env/API_URL");
  });

  it("deleteValue: DELETE /world-storage/values/{key}, no body", async () => {
    await deleteValue("highScore", opts);

    const { url, init, headers } = captured();
    expect(url).toBe(`${BASE}/world-storage/values/highScore`);
    expect(init.method).toBe("DELETE");
    expect(init.body).toBeUndefined();
    expect(headers.get("content-type")).toBeNull();
    expect(headers.get("x-confirm-delete-all")).toBeNull();
    expectSignedAs(headers, "DELETE", "/world-storage/values/highScore");
  });

  it("deleteEnvKey: DELETE /world-storage/env/{key}, no body", async () => {
    await deleteEnvKey("API_URL", opts);

    const { url, init, headers } = captured();
    expect(url).toBe(`${BASE}/world-storage/env/API_URL`);
    expect(init.method).toBe("DELETE");
    expect(init.body).toBeUndefined();
    expectSignedAs(headers, "DELETE", "/world-storage/env/API_URL");
  });

  it("clearValues: DELETE /world-storage/values with the confirm header", async () => {
    await clearValues(opts);

    const { url, init, headers } = captured();
    expect(url).toBe(`${BASE}/world-storage/values`);
    expect(init.method).toBe("DELETE");
    expect(init.body).toBeUndefined();
    expect(headers.get("x-confirm-delete-all")).toBe("true");
    expectSignedAs(headers, "DELETE", "/world-storage/values");
  });

  it("clearEnvKeys: DELETE /world-storage/env with the confirm header", async () => {
    await clearEnvKeys(opts);

    const { url, init, headers } = captured();
    expect(url).toBe(`${BASE}/world-storage/env`);
    expect(init.method).toBe("DELETE");
    expect(headers.get("x-confirm-delete-all")).toBe("true");
    expectSignedAs(headers, "DELETE", "/world-storage/env");
  });

  it("folds the world realm + parcel into the signed metadata", async () => {
    await deleteValue("k", opts);
    const meta = JSON.parse(captured().headers.get("x-identity-metadata") as string);
    expect(meta).toEqual({
      parcel: "1,2",
      realm: { serverName: "my-world.dcl.eth" },
      realmName: "my-world.dcl.eth",
    });
  });

  it("URL-encodes keys and signs the encoded path", async () => {
    await saveValue("a/b c", 1, opts);
    const { url, headers } = captured();
    expect(url).toBe(`${BASE}/world-storage/values/a%2Fb%20c`);
    expectSignedAs(headers, "PUT", "/world-storage/values/a%2Fb%20c");
  });

  it("forwards the abort signal", async () => {
    const controller = new AbortController();
    await saveValue("k", 1, { ...opts, signal: controller.signal });
    expect(captured().init.signal).toBe(controller.signal);
  });

  it("throws (never resolves) when the server rejects the write", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(null, { status: 403, statusText: "Forbidden" }),
    );
    await expect(deleteValue("k", opts)).rejects.toThrow(/403/);
  });

  it("throws when the network fetch fails", async () => {
    fetchMock.mockRejectedValueOnce(new Error("boom"));
    await expect(saveValue("k", 1, opts)).rejects.toThrow(/boom|failed/i);
  });
});
