import { describe, expect, it, vi } from "vitest";
import fixtures from "../../fixtures/places.json";
import { parsePlace, parsePlaces } from "./schema";

vi.mock("@core/lib/telemetry/track", () => ({ track: vi.fn() }));

const legacy = { ...fixtures.places.data[0], deployment_id: null };

describe("place API compatibility", () => {
  it("keeps records from peers that omit the additive ranking field but still rejects malformed ranking flags and other invalid fields", () => {
    const { exclude_from_ranking: _, ...row } = { ...legacy, exclude_from_ranking: false };
    expect(parsePlace(row)?.id).toBe(legacy.id);
    expect(parsePlaces([row, { ...row, exclude_from_ranking: true }])).toHaveLength(2);

    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      expect(parsePlace({ ...legacy, exclude_from_ranking: "false" })).toBeNull();
      expect(parsePlace({ ...legacy, id: 17 })).toBeNull();
    } finally { warn.mockRestore(); }
  });
});
