import { describe, expect, it } from "vitest";

import type { AllowedMethod } from "./auth-request-params";
import { MALICIOUS_REQUEST_TITLE, requestWarnings } from "./auth-request-warnings";

const TRUST_LINE = "Only continue if you trust the scene or app that asked for this.";

const WORDING: [AllowedMethod, string[]][] = [
  [
    "eth_sendTransaction",
    [
      "Move, sell or destroy any tokens, NFTs, LAND or names your wallet holds.",
      "Give another account permission to take your assets later, without asking you again.",
      "Once sent, it can't be undone.",
      TRUST_LINE,
    ],
  ],
  [
    "eth_signTypedData_v3",
    [
      "Authorize an order, a listing or a spending permission over your assets.",
      "Let a contract act for you later: a signature doesn't expire, and anyone who holds it can submit it.",
      "Log you in to another site or app as you.",
      TRUST_LINE,
    ],
  ],
  [
    "eth_signTypedData_v4",
    [
      "Authorize an order, a listing or a spending permission over your assets.",
      "Let a contract act for you later: a signature doesn't expire, and anyone who holds it can submit it.",
      "Log you in to another site or app as you.",
      TRUST_LINE,
    ],
  ],
  [
    "personal_sign",
    [
      "Log you in to another site or app as you.",
      "Authorize an order, a listing or a spending permission over your assets.",
      TRUST_LINE,
    ],
  ],
];

describe("requestWarnings", () => {
  it("says what a malicious request of each method could do and ends every list, without repeats, on trusting whoever asked", () => {
    for (const [method, expected] of WORDING) {
      const lines = requestWarnings(method);
      expect(lines, method).toEqual(expected);
      expect(lines.at(-1), method).toBe(TRUST_LINE);
      expect(new Set(lines).size, method).toBe(lines.length);
    }
  });

  it("heads the list with what the reader is being warned about", () => {
    expect(MALICIOUS_REQUEST_TITLE).toBe("If this request is malicious, it could:");
  });
});
