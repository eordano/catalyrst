import { describe, expect, it } from "vitest";

import {
  acknowledgmentBlocked,
  gatesOnPayloadReading,
  isScrolledToEnd,
} from "./auth-message-scroll";
import { approveBlocked } from "./auth-request-params";
import type { UnverifiableReason } from "./auth-request-params";

describe("isScrolledToEnd", () => {
  it("counts a block as read once it does not overflow or is scrolled to its end, fractional heights included", () => {
    const cases: [Parameters<typeof isScrolledToEnd>[0], boolean][] = [
      [{ scrollHeight: 120, scrollTop: 0, clientHeight: 220 }, true],
      [{ scrollHeight: 220, scrollTop: 0, clientHeight: 220 }, true],
      [{ scrollHeight: 2000, scrollTop: 0, clientHeight: 220 }, false],
      [{ scrollHeight: 2000, scrollTop: 1000, clientHeight: 220 }, false],
      [{ scrollHeight: 2000, scrollTop: 1780, clientHeight: 220 }, true],
      [{ scrollHeight: 2000.6, scrollTop: 1780, clientHeight: 220 }, true],
      [{ scrollHeight: 2000, scrollTop: 1900, clientHeight: 220 }, true],
    ];
    for (const [box, read] of cases) {
      expect(isScrolledToEnd(box), JSON.stringify(box)).toBe(read);
    }
  });
});

describe("gatesOnPayloadReading", () => {
  it("holds every classified request until its payload was read to the end and gates nothing before classification", () => {
    const reasons: UnverifiableReason[] = [
      "unverified_message",
      "opaque_message",
      "unrecognized_typed_data",
      "unsimulated_transaction",
    ];
    for (const unverifiable of reasons) {
      expect(gatesOnPayloadReading(unverifiable), unverifiable).toBe(true);
    }
    expect(gatesOnPayloadReading(null)).toBe(false);
  });
});

describe("acknowledgmentBlocked", () => {
  it("holds the acknowledgment only where the gate applies and the block is unread", () => {
    expect(acknowledgmentBlocked(true, false)).toBe(true);
    expect(acknowledgmentBlocked(true, true)).toBe(false);
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
