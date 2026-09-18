import { describe, expect, it, vi } from "vitest";

import { RPC_INVALID_PARAMS, RPC_METHOD_NOT_SUPPORTED } from "./auth-request-params";
import {
  isRequestExpired,
  parseRecoverResponse,
  recoverAuthRequest,
  type LoadResult,
  type RecoverResponse,
  type RecoveryDeps,
} from "./auth-request-recovery";

const ID = "123e4567-e89b-42d3-a456-426614174000";
const SENDER = "0x1234567890abcdef1234567890abcdef12345678";
const CONNECTED = "0x000000000000000000000000000000000000c0de";
const FUTURE = new Date(Date.now() + 5 * 60_000).toISOString();
const PAST = new Date(Date.now() - 60_000).toISOString();

const PERMIT = JSON.stringify({ primaryType: "Permit", domain: {}, types: {}, message: {} });

const META_TRANSACTION_WITHOUT_SALT_FIELD = JSON.stringify({
  types: {
    EIP712Domain: [
      { name: "name", type: "string" },
      { name: "version", type: "string" },
      { name: "verifyingContract", type: "address" },
    ],
    MetaTransaction: [
      { name: "nonce", type: "uint256" },
      { name: "from", type: "address" },
      { name: "functionData", type: "bytes" },
    ],
  },
  domain: {
    name: "DecentralandMarketplacePolygon",
    version: "1.0.0",
    verifyingContract: "0xa40b1d129b8906888720686f3a01921ddf37716f",
    salt: `0x${"00".repeat(31)}89`,
  },
  primaryType: "MetaTransaction",
  message: { nonce: 0, from: SENDER, functionData: `0xdeadbeef${"00".repeat(64)}` },
});

function request(overrides: Partial<RecoverResponse>): RecoverResponse {
  return {
    expiration: FUTURE,
    code: 42,
    method: "personal_sign",
    params: ["Sign in to Decentraland\nNonce: 1234", SENDER],
    sender: SENDER,
    challenge: "c",
    ...overrides,
  };
}

function deps(
  load: LoadResult,
  overrides: Partial<RecoveryDeps> = {},
): RecoveryDeps & { postOutcome: ReturnType<typeof vi.fn> } {
  return {
    load: vi.fn(async () => load),
    requiresValidation: vi.fn(async () => false),
    connectedAddress: vi.fn(async () => null),
    postOutcome: vi.fn(async () => ({ ok: true })),
    ...overrides,
  } as RecoveryDeps & { postOutcome: ReturnType<typeof vi.fn> };
}

describe("recoverAuthRequest", () => {
  it("passes a loader failure through without answering the request", async () => {
    for (const result of [
      { kind: "not_found" },
      { kind: "expired" },
      { kind: "fulfilled" },
      { kind: "error", message: "boom" },
    ] as LoadResult[]) {
      const d = deps(result);
      expect(await recoverAuthRequest(ID, d), result.kind).toEqual(result);
      expect(d.postOutcome, result.kind).not.toHaveBeenCalled();
    }
  });

  it("readies canonical, typed-data and MetaTransaction requests with their verification and acknowledgment needs", async () => {
    const canonical = deps({ kind: "ok", request: request({ method: "PERSONAL_SIGN" }) }, {
      requiresValidation: vi.fn(async () => true),
    });
    expect(await recoverAuthRequest(ID, canonical)).toMatchObject({
      kind: "ready",
      request: { method: "personal_sign", sender: SENDER, code: 42 },
      needsValidation: true,
      unverifiable: "unverified_message",
    });
    expect(canonical.postOutcome).not.toHaveBeenCalled();

    for (const typed of [PERMIT, META_TRANSACTION_WITHOUT_SALT_FIELD]) {
      const d = deps({
        kind: "ok",
        request: request({ method: "eth_signTypedData_v4", params: [SENDER, typed] }),
      });
      expect(await recoverAuthRequest(ID, d), typed).toMatchObject({
        kind: "ready",
        unverifiable: "unrecognized_typed_data",
      });
      expect(d.postOutcome, typed).not.toHaveBeenCalled();
    }
  });

  it("reports an unsupported method once with -32601 using the wallet's account, or the request's sender without one", async () => {
    const withWallet = deps(
      { kind: "ok", request: request({ method: "eth_sign", params: [SENDER, "0x00"] }) },
      { connectedAddress: vi.fn(async () => CONNECTED) },
    );
    expect(await recoverAuthRequest(ID, withWallet)).toMatchObject({
      kind: "rejected",
      reported: true,
      rejection: { code: RPC_METHOD_NOT_SUPPORTED, kind: "unsupported_method" },
    });
    expect(withWallet.postOutcome).toHaveBeenCalledTimes(1);
    expect(withWallet.postOutcome).toHaveBeenCalledWith(ID, {
      sender: CONNECTED,
      error: { code: RPC_METHOD_NOT_SUPPORTED, message: 'The "eth_sign" method is not supported' },
    });

    const retired = deps({ kind: "ok", request: request({ method: "dcl_personal_sign" }) });
    expect(await recoverAuthRequest(ID, retired)).toMatchObject({
      kind: "rejected",
      reported: true,
      rejection: { code: RPC_METHOD_NOT_SUPPORTED, kind: "retired_sign_in" },
    });
    expect(retired.postOutcome).toHaveBeenCalledTimes(1);
    expect(retired.postOutcome).toHaveBeenCalledWith(ID, expect.objectContaining({ sender: SENDER }));
  });

  it("reports malformed params once with -32602", async () => {
    const d = deps({
      kind: "ok",
      request: request({ method: "eth_sendTransaction", params: [{ to: "attacker.eth" }] }),
    });
    expect(await recoverAuthRequest(ID, d)).toMatchObject({
      kind: "rejected",
      reported: true,
      rejection: { code: RPC_INVALID_PARAMS, kind: "malformed_transaction" },
    });
    expect(d.postOutcome).toHaveBeenCalledTimes(1);
    expect(d.postOutcome.mock.calls[0]?.[1]).toMatchObject({
      error: { code: RPC_INVALID_PARAMS },
    });
  });

  it("holds the signature params to the request's sender, and only to the signer shape when it carries none", async () => {
    const mismatched = deps({ kind: "ok", request: request({ params: ["hello", CONNECTED] }) });
    expect(await recoverAuthRequest(ID, mismatched)).toMatchObject({
      kind: "rejected",
      rejection: { code: RPC_INVALID_PARAMS, kind: "malformed_signature" },
    });

    const senderless = deps({
      kind: "ok",
      request: request({ sender: undefined, params: ["hello", CONNECTED] }),
    });
    expect(await recoverAuthRequest(ID, senderless)).toMatchObject({ kind: "ready" });
  });

  it("leaves a rejected request unanswered when there is no sender at all, whitespace-only included", async () => {
    for (const sender of [undefined, "   "]) {
      const d = deps({ kind: "ok", request: request({ method: "eth_sign", sender }) });
      expect(await recoverAuthRequest(ID, d), String(sender)).toMatchObject({
        kind: "rejected",
        reported: false,
      });
      expect(d.postOutcome, String(sender)).not.toHaveBeenCalled();
    }
  });

  it("still rejects when the outcome post fails or throws", async () => {
    const failing = deps(
      { kind: "ok", request: request({ method: "eth_sign" }) },
      { postOutcome: vi.fn(async () => ({ ok: false })) },
    );
    expect(await recoverAuthRequest(ID, failing)).toMatchObject({ kind: "rejected", reported: false });

    const throwing = deps(
      { kind: "ok", request: request({ method: "eth_sign" }) },
      { postOutcome: vi.fn(async () => { throw new Error("offline"); }) },
    );
    expect(await recoverAuthRequest(ID, throwing)).toMatchObject({ kind: "rejected", reported: false });
  });

  it("treats a past expiration as expired before judging the method, by the injected clock when given", async () => {
    const past = deps({ kind: "ok", request: request({ method: "eth_sign", expiration: PAST }) });
    expect(await recoverAuthRequest(ID, past)).toEqual({ kind: "expired" });
    expect(past.postOutcome).not.toHaveBeenCalled();

    const clocked = deps({ kind: "ok", request: request({ expiration: FUTURE }) }, {
      now: () => Date.parse(FUTURE) + 1,
    });
    expect(await recoverAuthRequest(ID, clocked)).toEqual({ kind: "expired" });
    expect(clocked.postOutcome).not.toHaveBeenCalled();
  });
});

describe("isRequestExpired", () => {
  it("holds a request open until the instant it expires, reads an unparsable deadline as none, and defaults to now", () => {
    const at = Date.parse(FUTURE);
    expect(isRequestExpired(FUTURE, at - 1)).toBe(false);
    expect(isRequestExpired(FUTURE, at)).toBe(true);
    expect(isRequestExpired(FUTURE, at + 1)).toBe(true);
    expect(isRequestExpired("whenever", Date.now())).toBe(false);
    expect(isRequestExpired("", Date.now())).toBe(false);
    expect(isRequestExpired(PAST)).toBe(true);
    expect(isRequestExpired(FUTURE)).toBe(false);
  });
});

describe("parseRecoverResponse", () => {
  const body = {
    expiration: FUTURE,
    code: 42,
    method: "personal_sign",
    params: ["hello", SENDER],
    sender: SENDER,
    challenge: "c",
  };

  it("returns the record the auth server documents, the fields it omits left undefined", () => {
    expect(parseRecoverResponse(body)).toEqual(body);
    const { sender, challenge, params, ...rest } = body;
    expect(parseRecoverResponse(rest)).toEqual({
      ...rest,
      sender: undefined,
      challenge: undefined,
      params: undefined,
    });
  });

  it("refuses anything that is not the documented record", () => {
    const refused: [string, unknown][] = [
      ["nothing at all", null],
      ["a list", [body]],
      ["a bare string", "ok"],
      ["an expiration that is not a string", { ...body, expiration: 1_800_000 }],
      ["a code that is not a number", { ...body, code: "42" }],
      ["a method that is not a string", { ...body, method: { name: "personal_sign" } }],
      ["params that are not a list", { ...body, params: { 0: "hello" } }],
      ["a sender that is not a string", { ...body, sender: 12 }],
      ["a challenge that is not a string", { ...body, challenge: ["c"] }],
    ];
    for (const [label, value] of refused) {
      expect(parseRecoverResponse(value), label).toBeNull();
    }
  });
});
