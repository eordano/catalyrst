import { describe, expect, it } from "vitest";

import { roomIdentity, roomNameForPath } from "./room.server";

describe("roomNameForPath", () => {
  it("names a room after the page, trailing slash normalized", () => {
    expect(roomNameForPath("/foundry/gdd/flagrush-v1")).toBe(
      "foundry:/foundry/gdd/flagrush-v1",
    );
    expect(roomNameForPath("/foundry/play/")).toBe("foundry:/foundry/play");
    expect(roomNameForPath("/foundry")).toBe("foundry:/foundry");
  });

  it("refuses paths outside the foundry and junk", () => {
    for (const bad of ["/", "/admin", "/foundry/../etc", "/foundry/a b", "foundry"]) {
      expect(() => roomNameForPath(bad)).toThrow(/foundry pages/);
    }
  });
});

describe("roomIdentity", () => {
  it("is stable, sid-derived, and never the sid", () => {
    const sid = "sid-secret-value";
    expect(roomIdentity(sid)).toBe(roomIdentity(sid));
    expect(roomIdentity(sid)).toHaveLength(16);
    expect(roomIdentity(sid)).not.toContain("sid");
    expect(roomIdentity("other")).not.toBe(roomIdentity(sid));
  });
});
