import { describe, expect, it, vi } from "vitest";

import { loadMapJump } from "./map-jump.server";

import {
  PlaceRowSchema,
  rowToPin,
  filterPins,
  findPinByCoords,
  buildJumpUrl,
  parseCoords,
  normalizePinCategory,
  type MapPin,
} from "./map-jump";

function pinFromRaw(raw: unknown): MapPin {
  const parsed = PlaceRowSchema.parse(raw);
  return rowToPin(parsed);
}

describe("parseCoords", () => {
  it("parses x,y and tolerates junk", () => {
    expect(parseCoords("12,-42")).toEqual([12, -42]);
    expect(parseCoords("")).toEqual([0, 0]);
    expect(parseCoords("nope")).toEqual([0, 0]);
  });
});

describe("normalizePinCategory", () => {
  it("coerces to a known key, defaulting to all", () => {
    expect(normalizePinCategory("poi")).toBe("poi");
    expect(normalizePinCategory("POI")).toBe("poi");
    expect(normalizePinCategory("bogus")).toBe("all");
    expect(normalizePinCategory(null)).toBe("all");
  });
});

describe("rowToPin", () => {
  it("projects a live POI place onto a live pin", () => {
    const pin = pinFromRaw({
      id: "gp",
      title: "Genesis Plaza",
      base_position: "-3,-2",
      categories: ["poi"],
      user_count: 15,
      like_rate: 1.0,
      contact_name: "Decentraland Foundation",
      highlighted: true,
    });
    expect(pin).toMatchObject({
      id: "gp",
      name: "Genesis Plaza",
      coords: "-3,-2",
      x: -3,
      y: -2,
      category: "live",
      users: 15,
      rating: 100,
      live: true,
      featured: true,
      creator: "Decentraland Foundation",
    });
  });

  it("buckets a 0-player POI under poi and a plain place under people", () => {
    const poi = pinFromRaw({ id: "a", base_position: "0,0", categories: ["poi"], user_count: 0 });
    const people = pinFromRaw({ id: "b", base_position: "1,1", categories: [], user_count: 0 });
    const game = pinFromRaw({ id: "c", base_position: "2,2", categories: ["game"], user_count: 0 });
    expect(poi.category).toBe("poi");
    expect(people.category).toBe("people");
    expect(game.category).toBe("minigames");
  });

  it("falls back through contact_name -> owner -> empty for the creator", () => {
    const owner = pinFromRaw({ id: "a", base_position: "0,0", owner: "0xabc", contact_name: null });
    const none = pinFromRaw({ id: "b", base_position: "0,0" });
    expect(owner.creator).toBe("0xabc");
    expect(none.creator).toBe("");
  });
});

describe("filterPins", () => {
  const pins = [
    pinFromRaw({ id: "live", base_position: "0,0", user_count: 5 }),
    pinFromRaw({ id: "poi", base_position: "1,1", categories: ["poi"], user_count: 0 }),
    pinFromRaw({ id: "ppl", base_position: "2,2", categories: [], user_count: 0 }),
  ];
  it("passes all through for 'all'", () => {
    expect(filterPins(pins, "all")).toHaveLength(3);
  });
  it("filters by category", () => {
    expect(filterPins(pins, "poi").map((p) => p.id)).toEqual(["poi"]);
    expect(filterPins(pins, "live").map((p) => p.id)).toEqual(["live"]);
    expect(filterPins(pins, "people").map((p) => p.id)).toEqual(["ppl"]);
  });
});

describe("findPinByCoords", () => {
  const pins = [pinFromRaw({ id: "a", base_position: "12,-7" })];
  it("matches an exact coordinate, else null", () => {
    expect(findPinByCoords(pins, "12,-7")?.id).toBe("a");
    expect(findPinByCoords(pins, "0,0")).toBeNull();
    expect(findPinByCoords(pins, null)).toBeNull();
  });
});

describe("buildJumpUrl", () => {
  it("uses a LITERAL comma for genesis-city parcels (no %2C)", () => {
    const pin = pinFromRaw({ id: "a", base_position: "12,-7" });
    expect(buildJumpUrl(pin)).toBe("https://catalyst.example.com/play/?position=12,-7");
  });
  it("uses realm for Worlds", () => {
    const pin = pinFromRaw({
      id: "w",
      base_position: "0,0",
      world: true,
      world_name: "my-world.dcl.eth",
    });
    expect(buildJumpUrl(pin)).toBe("https://catalyst.example.com/play/?realm=my-world.dcl.eth");
  });
});

describe("loadMapJump", () => {
  const BASE = "http://places.test";
  function jsonResponse(body: unknown, status = 200): Response {
    return new Response(JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    });
  }

  it("reports an unavailable state on a non-2xx \u{2014} never substitute pins", async () => {
    const fetchImpl = vi.fn(async (_url: string) => jsonResponse({ error: "nope" }, 503));
    const data = await loadMapJump({ base: BASE, fetchImpl: fetchImpl as never });
    expect(data.source).toBe("unavailable");
    expect(data.pins).toEqual([]);
    expect(data.reason).toMatch(/503/);
  });

  it("reports an unavailable state when the endpoint is unreachable", async () => {
    const fetchImpl = vi.fn(async (_url: string) => {
      throw new Error("ECONNREFUSED");
    });
    const data = await loadMapJump({ base: BASE, fetchImpl: fetchImpl as never });
    expect(data.source).toBe("unavailable");
    expect(data.reason).toMatch(/ECONNREFUSED/);
  });

  it("keeps an empty live list as a live answer", async () => {
    const fetchImpl = vi.fn(async (_url: string) =>
      jsonResponse({ ok: true, data: [], total: 0 }),
    );
    const data = await loadMapJump({ base: BASE, fetchImpl: fetchImpl as never });
    expect(data.source).toBe("catalyst");
    expect(data.pins).toEqual([]);
  });
});
