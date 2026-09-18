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

describe("parseCoords / normalizePinCategory / findPinByCoords", () => {
  it("parses x,y tolerating junk, coerces categories to a known key (default all), and matches pins by exact coordinate", () => {
    expect(parseCoords("12,-42")).toEqual([12, -42]);
    expect(parseCoords("")).toEqual([0, 0]);
    expect(parseCoords("nope")).toEqual([0, 0]);

    expect(normalizePinCategory("poi")).toBe("poi");
    expect(normalizePinCategory("POI")).toBe("poi");
    expect(normalizePinCategory("bogus")).toBe("all");
    expect(normalizePinCategory(null)).toBe("all");

    const pins = [pinFromRaw({ id: "a", base_position: "12,-7" })];
    expect(findPinByCoords(pins, "12,-7")?.id).toBe("a");
    expect(findPinByCoords(pins, "0,0")).toBeNull();
    expect(findPinByCoords(pins, null)).toBeNull();
  });
});

describe("rowToPin", () => {
  it("projects a live POI onto a live pin, buckets 0-player POIs / plain places / games, and falls back contact_name -> owner -> empty for the creator", () => {
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

    const poi = pinFromRaw({ id: "a", base_position: "0,0", categories: ["poi"], user_count: 0 });
    const people = pinFromRaw({ id: "b", base_position: "1,1", categories: [], user_count: 0 });
    const game = pinFromRaw({ id: "c", base_position: "2,2", categories: ["game"], user_count: 0 });
    expect(poi.category).toBe("poi");
    expect(people.category).toBe("people");
    expect(game.category).toBe("minigames");

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
  it("passes all through for 'all' and filters by category", () => {
    expect(filterPins(pins, "all")).toHaveLength(3);
    expect(filterPins(pins, "poi").map((p) => p.id)).toEqual(["poi"]);
    expect(filterPins(pins, "live").map((p) => p.id)).toEqual(["live"]);
    expect(filterPins(pins, "people").map((p) => p.id)).toEqual(["ppl"]);
  });
});

describe("buildJumpUrl", () => {
  it("uses a LITERAL comma for genesis-city parcels (no %2C) and realm for Worlds", () => {
    const parcel = pinFromRaw({ id: "a", base_position: "12,-7" });
    expect(buildJumpUrl(parcel)).toBe("https://catalyst.example.com/play/?position=12,-7");

    const world = pinFromRaw({
      id: "w",
      base_position: "0,0",
      world: true,
      world_name: "my-world.dcl.eth",
    });
    expect(buildJumpUrl(world)).toBe("https://catalyst.example.com/play/?realm=my-world.dcl.eth");
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

  it("reports an unavailable state on a non-2xx or an unreachable endpoint \u{2014} never substitute pins", async () => {
    const failing = vi.fn(async (_url: string) => jsonResponse({ error: "nope" }, 503));
    const nonOk = await loadMapJump({ base: BASE, fetchImpl: failing as never });
    expect(nonOk.source).toBe("unavailable");
    expect(nonOk.pins).toEqual([]);
    expect(nonOk.reason).toMatch(/503/);

    const unreachable = vi.fn(async (_url: string) => {
      throw new Error("ECONNREFUSED");
    });
    const down = await loadMapJump({ base: BASE, fetchImpl: unreachable as never });
    expect(down.source).toBe("unavailable");
    expect(down.reason).toMatch(/ECONNREFUSED/);
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
