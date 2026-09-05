import { describe, expect, it, vi } from "vitest";

import { RPC_INVALID_PARAMS, RPC_METHOD_NOT_SUPPORTED } from "./auth-request-params";
import {
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
      expect(await recoverAuthRequest(ID, d)).toEqual(result);
      expect(d.postOutcome).not.toHaveBeenCalled();
    }
  });

  it("readies a canonical request with its verification and acknowledgment needs", async () => {
    const d = deps({ kind: "ok", request: request({ method: "PERSONAL_SIGN" }) }, {
      requiresValidation: vi.fn(async () => true),
    });
    const outcome = await recoverAuthRequest(ID, d);
    expect(outcome).toMatchObject({
      kind: "ready",
      request: { method: "personal_sign", sender: SENDER, code: 42 },
      needsValidation: true,
      unverifiable: null,
    });
    expect(d.postOutcome).not.toHaveBeenCalled();
  });

  it("flags a typed-data request for the effects acknowledgment", async () => {
    const typed = JSON.stringify({ primaryType: "Permit", domain: {}, types: {}, message: {} });
    const d = deps({
      kind: "ok",
      request: request({ method: "eth_signTypedData_v4", params: [SENDER, typed] }),
    });
    expect(await recoverAuthRequest(ID, d)).toMatchObject({
      kind: "ready",
      unverifiable: "unrecognized_typed_data",
    });
  });

  it("reports an unsupported method once with -32601 using the wallet's account", async () => {
    const d = deps(
      { kind: "ok", request: request({ method: "eth_sign", params: [SENDER, "0x00"] }) },
      { connectedAddress: vi.fn(async () => CONNECTED) },
    );
    const outcome = await recoverAuthRequest(ID, d);
    expect(outcome).toMatchObject({
      kind: "rejected",
      reported: true,
      rejection: { code: RPC_METHOD_NOT_SUPPORTED, kind: "unsupported_method" },
    });
    expect(d.postOutcome).toHaveBeenCalledTimes(1);
    expect(d.postOutcome).toHaveBeenCalledWith(ID, {
      sender: CONNECTED,
      error: { code: RPC_METHOD_NOT_SUPPORTED, message: 'The "eth_sign" method is not supported' },
    });
  });

  it("falls back to the request's sender when no wallet account is available", async () => {
    const d = deps({ kind: "ok", request: request({ method: "dcl_personal_sign" }) });
    expect(await recoverAuthRequest(ID, d)).toMatchObject({
      kind: "rejected",
      reported: true,
      rejection: { code: RPC_METHOD_NOT_SUPPORTED, kind: "retired_sign_in" },
    });
    expect(d.postOutcome).toHaveBeenCalledWith(ID, expect.objectContaining({ sender: SENDER }));
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

  it("holds the signature params to the request's sender at recover", async () => {
    const d = deps({
      kind: "ok",
      request: request({ params: ["hello", CONNECTED] }),
    });
    expect(await recoverAuthRequest(ID, d)).toMatchObject({
      kind: "rejected",
      rejection: { code: RPC_INVALID_PARAMS, kind: "malformed_signature" },
    });
  });

  it("only checks the signer shape when the request carries no sender", async () => {
    const d = deps({
      kind: "ok",
      request: request({ sender: undefined, params: ["hello", CONNECTED] }),
    });
    expect(await recoverAuthRequest(ID, d)).toMatchObject({ kind: "ready" });
  });

  it("leaves a rejected request unanswered when there is no sender at all", async () => {
    const d = deps({ kind: "ok", request: request({ method: "eth_sign", sender: undefined }) });
    expect(await recoverAuthRequest(ID, d)).toMatchObject({ kind: "rejected", reported: false });
    expect(d.postOutcome).not.toHaveBeenCalled();
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

  it("treats a past expiration as expired before judging the method", async () => {
    const d = deps({ kind: "ok", request: request({ method: "eth_sign", expiration: PAST }) });
    expect(await recoverAuthRequest(ID, d)).toEqual({ kind: "expired" });
    expect(d.postOutcome).not.toHaveBeenCalled();
  });

  it("uses the injected clock for the expiration check", async () => {
    const d = deps({ kind: "ok", request: request({ expiration: FUTURE }) }, {
      now: () => Date.parse(FUTURE) + 1,
    });
    expect(await recoverAuthRequest(ID, d)).toEqual({ kind: "expired" });
  });
});
