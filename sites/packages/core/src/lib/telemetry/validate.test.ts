import { describe, expect, it } from "vitest";
import generatedContract from "./telemetry-contract.json";

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
  it("accepts correct events, present or absent optionals, extras, unknown-kind values and loose events", () => {
    expect(validateEventAgainst(C, "cl_x_shown", good)).toEqual([]);
    expect(validateEventAgainst(C, "cl_x_shown", { ...good, note: "hi", slot: 2 })).toEqual([]);
    expect(validateEventAgainst(C, "cl_x_shown", { ...good, extra: 1 })).toEqual([]);
    expect(validateEventAgainst(C, "cl_x_shown", { ...good, payload: { deep: true } })).toEqual([]);
    expect(validateEventAgainst(C, "cl_empty", { extra: 1 })).toEqual([]);
    expect(validateEventAgainst(C, "cl_empty", null)).toEqual([]);
    expect(validateEventAgainst(C, "cl_loose", { anything: [1, 2], nested: { a: 1 } })).toEqual([]);
  });

  it("flags an unknown event and a missing required prop, treating null props as empty", () => {
    expect(validateEventAgainst(C, "nope", {})[0]).toMatch(/unknown event/);
    expect(validateEventAgainst(C, "cl_x_shown", { count: 1, ok: true })[0]).toMatch(
      /missing required prop "variant"/,
    );
    expect(validateEventAgainst(C, "cl_x_shown", null).length).toBeGreaterThan(0);
  });

  it("flags wrong primitive, optional and enum kinds", () => {
    expect(validateEventAgainst(C, "cl_x_shown", { ...good, count: "3" }).join()).toMatch(
      /prop "count" should be number, got string/,
    );
    expect(validateEventAgainst(C, "cl_x_shown", { ...good, note: 5 }).join()).toMatch(
      /prop "note" should be string/,
    );
    expect(validateEventAgainst(C, "cl_x_shown", { ...good, variant: "z" }).join()).toMatch(
      /not one of \{a, b\}/,
    );
    expect(validateEventAgainst(C, "cl_x_shown", { ...good, slot: 9 }).join()).toMatch(
      /not one of \{1, 2, 3\}/,
    );
  });
});


describe("production nullable creator events", () => {
  it.each([
    ["ch_manage_viewed", {
      address: null, search: null, sort: "domain", filter: "published", count: 0,
    }],
    ["creator_dashboard_viewed", {
      on_sale_items: null, published_collections: null, sales_7d: null,
      scene_visits_30d: null, window_days: 7,
    }],
  ] as [string, Record<string, unknown>][])(
    "accepts explicit nulls but still requires each property for %s",
    (event, props) => {
      expect(validateEventAgainst(generatedContract, event, props)).toEqual([]);
      for (const key of Object.keys(props)) {
        const absent = { ...props };
        delete absent[key];
        expect(validateEventAgainst(generatedContract, event, absent))
          .toContain(`missing required prop "${key}"`);
      }
    },
  );
});
