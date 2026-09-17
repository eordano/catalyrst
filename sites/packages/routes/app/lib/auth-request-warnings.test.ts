import { describe, expect, it } from "vitest";

import type { AllowedMethod } from "./auth-request-params";
import { MALICIOUS_REQUEST_TITLE, requestWarnings } from "./auth-request-warnings";

const METHODS: AllowedMethod[] = [
  "personal_sign",
  "eth_signTypedData_v3",
  "eth_signTypedData_v4",
  "eth_sendTransaction",
];

describe("requestWarnings", () => {
  it("says what a malicious transaction could do with the account it runs from", () => {
    expect(requestWarnings("eth_sendTransaction")).toEqual([
      "Move, sell or destroy any tokens, NFTs, LAND or names your wallet holds.",
      "Give another account permission to take your assets later, without asking you again.",
      "Once sent, it can't be undone.",
      "Only continue if you trust the scene or app that asked for this.",
    ]);
  });

  it("says a signature is a bearer authorization that does not expire", () => {
    for (const method of ["eth_signTypedData_v3", "eth_signTypedData_v4"] as const) {
      expect(requestWarnings(method)).toEqual([
        "Authorize an order, a listing or a spending permission over your assets.",
        "Let a contract act for you later: a signature doesn't expire, and anyone who holds it can submit it.",
        "Log you in to another site or app as you.",
        "Only continue if you trust the scene or app that asked for this.",
      ]);
    }
  });

  it("says a signed message can be a login or an off-chain order", () => {
    expect(requestWarnings("personal_sign")).toEqual([
      "Log you in to another site or app as you.",
      "Authorize an order, a listing or a spending permission over your assets.",
      "Only continue if you trust the scene or app that asked for this.",
    ]);
  });

  it.each(METHODS)("ends %s on trusting whoever asked", (method) => {
    const lines = requestWarnings(method);
    expect(lines.at(-1)).toBe("Only continue if you trust the scene or app that asked for this.");
    expect(new Set(lines).size).toBe(lines.length);
  });

  it("heads the list with what the reader is being warned about", () => {
    expect(MALICIOUS_REQUEST_TITLE).toBe("If this request is malicious, it could:");
  });
});
