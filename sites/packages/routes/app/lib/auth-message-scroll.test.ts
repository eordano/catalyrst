import { describe, expect, it } from "vitest";

import {
  acknowledgmentBlocked,
  gatesOnPayloadReading,
  isScrolledToEnd,
} from "./auth-message-scroll";
import { approveBlocked } from "./auth-request-params";
import type { UnverifiableReason } from "./auth-request-params";

describe("isScrolledToEnd", () => {
  it("counts a block that does not overflow as read", () => {
    expect(isScrolledToEnd({ scrollHeight: 120, scrollTop: 0, clientHeight: 220 })).toBe(true);
    expect(isScrolledToEnd({ scrollHeight: 220, scrollTop: 0, clientHeight: 220 })).toBe(true);
  });

  it("holds a long message until it has been scrolled to its end", () => {
    expect(isScrolledToEnd({ scrollHeight: 2000, scrollTop: 0, clientHeight: 220 })).toBe(false);
    expect(isScrolledToEnd({ scrollHeight: 2000, scrollTop: 1000, clientHeight: 220 })).toBe(false);
    expect(isScrolledToEnd({ scrollHeight: 2000, scrollTop: 1777, clientHeight: 220 })).toBe(false);
    expect(isScrolledToEnd({ scrollHeight: 2000, scrollTop: 1780, clientHeight: 220 })).toBe(true);
  });

  it("allows the two pixels a zoomed or high-density screen leaves behind", () => {
    expect(isScrolledToEnd({ scrollHeight: 400, scrollTop: 198, clientHeight: 200 })).toBe(true);
    expect(isScrolledToEnd({ scrollHeight: 400, scrollTop: 197, clientHeight: 200 })).toBe(false);
    expect(isScrolledToEnd({ scrollHeight: 2000.6, scrollTop: 1780, clientHeight: 220 })).toBe(true);
    expect(isScrolledToEnd({ scrollHeight: 2003, scrollTop: 1780, clientHeight: 220 })).toBe(false);
  });

  it("stays true once the block is scrolled past its end", () => {
    expect(isScrolledToEnd({ scrollHeight: 2000, scrollTop: 1900, clientHeight: 220 })).toBe(true);
  });
});

describe("gatesOnPayloadReading", () => {
  const rows: [string, UnverifiableReason][] = [
    ["a readable personal_sign message", "unverified_message"],
    ["an opaque personal_sign message", "opaque_message"],
    ["typed data", "unrecognized_typed_data"],
    ["a transaction", "unsimulated_transaction"],
  ];

  it.each(rows)("holds %s until its payload was read to the end", (_label, unverifiable) => {
    expect(gatesOnPayloadReading(unverifiable)).toBe(true);
  });

  it("gates nothing before the request has been classified", () => {
    expect(gatesOnPayloadReading(null)).toBe(false);
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

describe("what Approve is held on", () => {
  const gates = {
    isSigning: false,
    mustValidate: false,
    acknowledged: false,
    unverifiable: "unrecognized_typed_data" as UnverifiableReason,
    effectsAcknowledged: true,
  };

  it("holds Approve on the payload gate of its own, not only through the checkbox", () => {
    expect(approveBlocked(gates)).toBe(false);
    const gatesOnReading = gatesOnPayloadReading(gates.unverifiable);
    expect(approveBlocked(gates) || acknowledgmentBlocked(gatesOnReading, false)).toBe(true);
    expect(approveBlocked(gates) || acknowledgmentBlocked(gatesOnReading, true)).toBe(false);
  });
});
