import { describe, expect, it } from "vitest";

import type { AllowedMethod } from "./auth-request-params";
import { requestWarnings } from "./auth-request-warnings";

const RISKY_METHODS: AllowedMethod[] = [
  "eth_sendTransaction",
  "eth_signTypedData_v3",
  "eth_signTypedData_v4",
  "personal_sign",
];

describe("requestWarnings", () => {
  it("warns about every method a request can carry and ends every list, without repeats, on trusting whoever asked", () => {
    for (const method of RISKY_METHODS) {
      const lines = requestWarnings(method);
      expect(lines.length, method).toBeGreaterThan(1);
      for (const line of lines) {
        expect(line.trim().length, method).toBeGreaterThan(0);
      }
      expect(lines.at(-1), method).toMatch(/trust/i);
      expect(new Set(lines).size, method).toBe(lines.length);
    }
  });
});
