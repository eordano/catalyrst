import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../auth/signer", () => ({
  signedFetch: vi.fn(),
}));

import { signedFetch } from "../auth/signer";
import { CatalystError, postJSON, signedGetJSON } from "./client";
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

describe("postJSON \u{2014} server error-message surfacing (checkout error root cause)", () => {
  it("surfaces the catalyrst envelope's message on 409 with status intact", async () => {
    mSignedFetch.mockResolvedValue(
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
  });

  it("surfaces server messages for non-409 errors too (402, 500, \u{2026})", async () => {
    mSignedFetch.mockResolvedValue(
      jsonResponse(402, { ok: false, message: "insufficient credits balance" }),
    );
    const err = await catchErr(postJSON("/credits/checkout", {}, { identity: IDENTITY }));
    expect(err.status).toBe(402);
    expect(err.message).toBe("insufficient credits balance");
    expect(err.serverMessage).toBe(true);
  });

  it("uses only the message field \u{2014} other body fields never leak", async () => {
    mSignedFetch.mockResolvedValue(
      jsonResponse(500, {
        ok: false,
        message: "database error",
        detail: "connection to 10.0.0.5:5434 refused (secret-host)",
        stack: "at very::internal::frame",
      }),
    );
    const err = await catchErr(postJSON("/credits/checkout", {}, { identity: IDENTITY }));
    expect(err.message).toBe("database error");
    expect(err.message).not.toContain("secret-host");
    expect(err.message).not.toContain("internal::frame");
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

describe("signedGetJSON \u{2014} same surfacing on authenticated reads", () => {
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
