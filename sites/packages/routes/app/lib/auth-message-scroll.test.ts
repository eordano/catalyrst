import { describe, expect, it } from "vitest";

import {
  acknowledgmentBlocked,
  gatesOnMessageReading,
  isScrolledToEnd,
} from "./auth-message-scroll";
import type { AllowedMethod, UnverifiableReason } from "./auth-request-params";

// The measurement auth's UnverifiedRequestView makes on its message block (messageReadToEnd): the
// acknowledgment opens only once the block has nothing left below the fold.
describe("isScrolledToEnd", () => {
  it("counts a block that does not overflow as read", () => {
    expect(isScrolledToEnd({ scrollHeight: 120, scrollTop: 0, clientHeight: 220 })).toBe(true);
    expect(isScrolledToEnd({ scrollHeight: 220, scrollTop: 0, clientHeight: 220 })).toBe(true);
  });

  it("holds a long message until it has been scrolled to its end", () => {
    expect(isScrolledToEnd({ scrollHeight: 2000, scrollTop: 0, clientHeight: 220 })).toBe(false);
    expect(isScrolledToEnd({ scrollHeight: 2000, scrollTop: 1000, clientHeight: 220 })).toBe(false);
    expect(isScrolledToEnd({ scrollHeight: 2000, scrollTop: 1778, clientHeight: 220 })).toBe(false);
    expect(isScrolledToEnd({ scrollHeight: 2000, scrollTop: 1780, clientHeight: 220 })).toBe(true);
  });

  // Sub-pixel layout leaves a fraction of a pixel below a block scrolled all the way down, which is
  // why the end is measured to the pixel: without the tolerance such a block would never open.
  it("allows the fraction of a pixel sub-pixel layout leaves behind", () => {
    expect(isScrolledToEnd({ scrollHeight: 2000.6, scrollTop: 1780, clientHeight: 220 })).toBe(true);
    expect(isScrolledToEnd({ scrollHeight: 2002, scrollTop: 1780, clientHeight: 220 })).toBe(false);
  });

  it("stays true once the block is scrolled past its end", () => {
    expect(isScrolledToEnd({ scrollHeight: 2000, scrollTop: 1900, clientHeight: 220 })).toBe(true);
  });
});

// The scoping upstream's UnverifiedRequestView gives the same gate (`messageText !== null`): the
// request classes whose detail box is a message somebody reads, and no others.
describe("gatesOnMessageReading", () => {
  const rows: [string, AllowedMethod, UnverifiableReason, boolean][] = [
    ["a readable personal_sign message", "personal_sign", "unverified_message", true],
    ["an opaque personal_sign message", "personal_sign", "opaque_message", false],
    ["typed data", "eth_signTypedData_v4", "unrecognized_typed_data", false],
    ["typed data sent as v3", "eth_signTypedData_v3", "unrecognized_typed_data", false],
    ["a transaction", "eth_sendTransaction", "unsimulated_transaction", false],
  ];

  it.each(rows)("gates %s: %s", (_label, method, unverifiable, expected) => {
    expect(gatesOnMessageReading(method, unverifiable)).toBe(expected);
  });

  it("gates nothing before the request has been classified", () => {
    expect(gatesOnMessageReading("personal_sign", null)).toBe(false);
  });
});

describe("acknowledgmentBlocked", () => {
  it("holds the acknowledgment only where the gate applies and the block is unread", () => {
    expect(acknowledgmentBlocked(true, false)).toBe(true);
    expect(acknowledgmentBlocked(true, true)).toBe(false);
  });

  it("never holds a request the gate does not apply to", () => {
    expect(acknowledgmentBlocked(false, false)).toBe(false);
    expect(acknowledgmentBlocked(false, true)).toBe(false);
  });
});
