import { describe, expect, it } from "vitest";

import { validateEventAgainst, type TelemetryContract } from "./validate";

const C: TelemetryContract = {
  events: {
    cl_x_shown: {
      loose: false,
      props: {
        variant: { kind: "enum-string", values: ["a", "b"], optional: false },
        count: { kind: "number", optional: false },
        note: { kind: "string", optional: true },
        slot: { kind: "enum-number", values: [1, 2, 3], optional: true },
        ok: { kind: "boolean", optional: false },
        payload: { kind: "unknown", optional: true },
      },
    },
    cl_loose: { loose: true, props: {} },
    cl_empty: { loose: false, props: {} },
  },
};

const good = { variant: "a", count: 3, ok: true };

describe("validateEventAgainst", () => {
  it("passes a fully correct event", () => {
    expect(validateEventAgainst(C, "cl_x_shown", good)).toEqual([]);
  });

  it("flags an unknown event", () => {
    expect(validateEventAgainst(C, "nope", {})[0]).toMatch(/unknown event/);
  });

  it("flags a missing required prop", () => {
    expect(validateEventAgainst(C, "cl_x_shown", { count: 1, ok: true })[0]).toMatch(
      /missing required prop "variant"/,
    );
  });

  it("flags a wrong primitive kind", () => {
    expect(
      validateEventAgainst(C, "cl_x_shown", { ...good, count: "3" }).join(),
    ).toMatch(/prop "count" should be number, got string/);
  });

  it("flags an enum-string value not in the set", () => {
    expect(
      validateEventAgainst(C, "cl_x_shown", { ...good, variant: "z" }).join(),
    ).toMatch(/not one of \{a, b\}/);
  });

  it("flags an enum-number value not in the set", () => {
    expect(
      validateEventAgainst(C, "cl_x_shown", { ...good, slot: 9 }).join(),
    ).toMatch(/not one of \{1, 2, 3\}/);
  });

  it("accepts an absent optional prop and a valid present one", () => {
    expect(validateEventAgainst(C, "cl_x_shown", { ...good, note: "hi", slot: 2 })).toEqual([]);
  });

  it("still checks an optional prop's type when present", () => {
    expect(
      validateEventAgainst(C, "cl_x_shown", { ...good, note: 5 }).join(),
    ).toMatch(/prop "note" should be string/);
  });

  it("accepts any props for a loose event", () => {
    expect(validateEventAgainst(C, "cl_loose", { anything: [1, 2], nested: { a: 1 } })).toEqual([]);
  });

  it("is lenient about extra props not in the contract", () => {
    expect(validateEventAgainst(C, "cl_empty", { extra: 1 })).toEqual([]);
    expect(validateEventAgainst(C, "cl_x_shown", { ...good, extra: 1 })).toEqual([]);
  });

  it("accepts unknown-kind props regardless of value", () => {
    expect(validateEventAgainst(C, "cl_x_shown", { ...good, payload: { deep: true } })).toEqual([]);
  });

  it("treats null props as empty (missing-required reported)", () => {
    expect(validateEventAgainst(C, "cl_x_shown", null).length).toBeGreaterThan(0);
    expect(validateEventAgainst(C, "cl_empty", null)).toEqual([]);
  });
});
