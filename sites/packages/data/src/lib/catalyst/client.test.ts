import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../auth/signer", () => ({
  signedFetch: vi.fn(),
}));

import { signedFetch } from "../auth/signer";
import { CatalystError, getJSON, postJSON, signedGetJSON } from "./client";
import type { AuthIdentity } from "../auth/types";

const mSignedFetch = vi.mocked(signedFetch);

const IDENTITY: AuthIdentity = {
  signer: "0x4e9c4a2502fdf71e93ed8ed6ca9ddbd891d6f295",
  ephemeral: { address: "0xeph", privateKey: "0xdeadbeef" },
  expiration: "2999-01-01T00:00:00.000Z",
  authChain: [],
};

function jsonResponse(status: number, body: unknown, statusText = ""): Response {
  return new Response(JSON.stringify(body), {
    status,
    statusText,
    headers: { "content-type": "application/json" },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

async function catchErr(p: Promise<unknown>): Promise<CatalystError> {
  try {
    await p;
  } catch (e) {
    return e as CatalystError;
  }
  throw new Error("expected the request to reject");
}

describe("postJSON server error-message surfacing (checkout error root cause)", () => {
  it("surfaces only the envelope's message with status intact, on 409 and any other status", async () => {
    mSignedFetch.mockResolvedValueOnce(
      jsonResponse(409, {
        ok: false,
        message:
          "the order total changed after the purchase was signed (signed 3, current 4) \u{2014} please review and sign again",
      }),
    );
    const err = await catchErr(postJSON("/credits/checkout", {}, { identity: IDENTITY }));
    expect(err).toBeInstanceOf(CatalystError);
    expect(err.status).toBe(409);
    expect(err.serverMessage).toBe(true);
    expect(err.message).toContain("the order total changed after the purchase was signed");
    expect(err.message).not.toMatch(/Catalyst returned/);

    mSignedFetch.mockResolvedValueOnce(
      jsonResponse(402, { ok: false, message: "insufficient credits balance" }),
    );
    const paid = await catchErr(postJSON("/credits/checkout", {}, { identity: IDENTITY }));
    expect(paid.status).toBe(402);
    expect(paid.message).toBe("insufficient credits balance");
    expect(paid.serverMessage).toBe(true);

    mSignedFetch.mockResolvedValueOnce(
      jsonResponse(500, {
        ok: false,
        message: "database error",
        detail: "connection to 10.0.0.5:5434 refused (secret-host)",
        stack: "at very::internal::frame",
      }),
    );
    const leak = await catchErr(postJSON("/credits/checkout", {}, { identity: IDENTITY }));
    expect(leak.message).toBe("database error");
    expect(leak.message).not.toContain("secret-host");
    expect(leak.message).not.toContain("internal::frame");
  });

  it("falls back to the generic message when the body is not JSON or has no message", async () => {
    mSignedFetch.mockResolvedValue(
      new Response("<html>bad gateway</html>", { status: 502, statusText: "Bad Gateway" }),
    );
    const err = await catchErr(postJSON("/credits/checkout", {}, { identity: IDENTITY }));
    expect(err.message).toBe("Catalyst returned 502 Bad Gateway");
    expect(err.serverMessage).toBe(false);

    mSignedFetch.mockResolvedValue(jsonResponse(409, { ok: false, message: "   " }));
    const err2 = await catchErr(postJSON("/credits/checkout", {}, { identity: IDENTITY }));
    expect(err2.message).toMatch(/Catalyst returned 409/);
    expect(err2.serverMessage).toBe(false);
  });
});

describe("getJSON per-request in-flight dedupe", () => {
  it("identical concurrent reads under one signal share a single upstream fetch", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse(200, { realm: "hela" }));
    const { signal } = new AbortController();
    const [a, b] = await Promise.all([
      getJSON<{ realm: string }>("/about", { fetchImpl, signal }),
      getJSON<{ realm: string }>("/about", { fetchImpl, signal }),
    ]);
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(a).toEqual({ realm: "hela" });
    expect(b).toEqual(a);
    expect(b).not.toBe(a);
  });

  it("does not share across signals, without a signal, or once the read settled", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse(200, {}));
    const { signal } = new AbortController();
    const other = new AbortController().signal;
    await Promise.all([
      getJSON("/about", { fetchImpl, signal }),
      getJSON("/about", { fetchImpl, signal: other }),
      getJSON("/about", { fetchImpl }),
    ]);
    expect(fetchImpl).toHaveBeenCalledTimes(3);
    await getJSON("/about", { fetchImpl, signal });
    expect(fetchImpl).toHaveBeenCalledTimes(4);
  });

  it("a differing query or header is a different read", async () => {
    const fetchImpl = vi.fn(async () => jsonResponse(200, {}));
    const { signal } = new AbortController();
    await Promise.all([
      getJSON("/x", { fetchImpl, signal, query: { a: 1 } }),
      getJSON("/x", { fetchImpl, signal, query: { a: 2 } }),
      getJSON("/x", { fetchImpl, signal, query: { a: 1 }, headers: { "x-k": "v" } }),
    ]);
    expect(fetchImpl).toHaveBeenCalledTimes(3);
  });

  it("every sharer sees the same CatalystError", async () => {
    const fetchImpl = vi.fn(async () => new Response("nope", { status: 502, statusText: "Bad Gateway" }));
    const { signal } = new AbortController();
    const [a, b] = await Promise.all([
      catchErr(getJSON("/about", { fetchImpl, signal })),
      catchErr(getJSON("/about", { fetchImpl, signal })),
    ]);
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(a.status).toBe(502);
    expect(a.message).toBe("Catalyst returned 502 Bad Gateway");
    expect(b).toBe(a);
  });

  it("keeps the network and invalid-JSON error shapes", async () => {
    const down = vi.fn(async () => {
      throw new Error("ECONNREFUSED");
    });
    const err = await catchErr(getJSON("/about", { fetchImpl: down }));
    expect(err.status).toBe(0);
    expect(err.message).toBe("Catalyst request failed: ECONNREFUSED");

    const bad = vi.fn(async () => new Response("<html>", { status: 200 }));
    const err2 = await catchErr(getJSON("/about", { fetchImpl: bad }));
    expect(err2.status).toBe(200);
    expect(err2.message).toMatch(/^Catalyst returned invalid JSON/);
  });
});

describe("signedGetJSON surfaces the server message on authenticated reads", () => {
  it("surfaces the server message", async () => {
    mSignedFetch.mockResolvedValue(
      jsonResponse(403, { ok: false, message: "checkout does not belong to signer" }),
    );
    const err = await catchErr(signedGetJSON("/credits/checkout/41", { identity: IDENTITY }));
    expect(err).toBeInstanceOf(CatalystError);
    expect(err.status).toBe(403);
    expect(err.message).toBe("checkout does not belong to signer");
  });
});
