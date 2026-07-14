import { describe, expect, it } from "vitest";

import { type Place } from "@data/lib/catalyst/places/index";
import { selectLiveTargets, toOpenPlace } from "./select";

// Only the fields toOpenPlace/selectLiveTargets read; the rest of Place is
// irrelevant to selection and never parsed at runtime here.
function p(id: string, user_count: number | null, title: string | null = id): Place {
  return { id, title, base_position: "0,0", user_count } as unknown as Place;
}

describe("toOpenPlace (schema-honesty predicate)", () => {
  it("returns null for a missing or non-positive reading (never a fabricated live place)", () => {
    expect(toOpenPlace(null)).toBeNull();
    expect(toOpenPlace(undefined)).toBeNull();
    expect(toOpenPlace(p("x", null))).toBeNull();
    expect(toOpenPlace(p("x", 0))).toBeNull();
    expect(toOpenPlace(p("x", -3))).toBeNull();
  });

  it("projects a positive reading and drops fields the arms never render", () => {
    const op = toOpenPlace(p("plc", 42, "Plaza"));
    expect(op).toEqual({ id: "plc", title: "Plaza", base_position: "0,0", user_count: 42 });
    expect(op).not.toHaveProperty("image");
  });
});

describe("selectLiveTargets", () => {
  it("returns nulls when nothing is live (empty, null, or all non-positive)", () => {
    expect(selectLiveTargets([])).toEqual({ busiest: null, surprise: null });
    expect(selectLiveTargets(null)).toEqual({ busiest: null, surprise: null });
    expect(selectLiveTargets([p("a", 0), p("b", null), p("c", -1)])).toEqual({
      busiest: null,
      surprise: null,
    });
  });

  it("picks the highest user_count as busiest, ignoring dead places", () => {
    const { busiest } = selectLiveTargets([p("a", 5), p("b", 99), p("c", 0), p("d", 40)]);
    expect(busiest?.id).toBe("b");
    expect(busiest?.user_count).toBe(99);
  });

  it("surprise is a DISTINCT live place, chosen via the injected rng", () => {
    const places = [p("busy", 99), p("x", 3), p("y", 7), p("z", 1)];
    // others (busiest 'busy' removed) = [x, y, z]; rng 0 -> first, ->~1 -> last.
    expect(selectLiveTargets(places, () => 0).surprise?.id).toBe("x");
    expect(selectLiveTargets(places, () => 0.999).surprise?.id).toBe("z");
    const { busiest, surprise } = selectLiveTargets(places, () => 0.5);
    expect(surprise?.id).not.toBe(busiest?.id);
  });

  it("falls back to the busiest as surprise when it is the only live place", () => {
    const { busiest, surprise } = selectLiveTargets([p("solo", 12), p("dead", 0)]);
    expect(busiest?.id).toBe("solo");
    expect(surprise?.id).toBe("solo");
  });
});
