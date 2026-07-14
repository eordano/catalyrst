import { it, expect, beforeAll } from "vitest";

import { describeRequiringPg } from "./require-dep";

import { dbSource } from "../../packages/data/src/lib/catalyst/db.server";
import {
  loadPlaces,
  loadPlace,
  loadCategories,
} from "../../packages/data/src/lib/catalyst/places/index.server";
import { mapPlace } from "../../packages/data/src/lib/catalyst/places/index";
import { loadFixtureEntries } from "./seed";

const d = describeRequiringPg();

const entries = loadFixtureEntries();
const SEEDED = entries.length;

const AUDIO_HOT_LAB = "830d885b-52f3-4c91-9151-9c8ec40aab63";
const GENESIS_PLAZA = "3ca25728-5b41-48f6-8e24-5cb0e3f2bb5d";

d("Places DB path (temp postgres from fixture)", () => {
  beforeAll(() => {
    expect(process.env.CATALYST_DATABASE_URL).toBeTruthy();
  });

  it("dbSource.fetchPlaces returns every seeded place, like_score-ordered", async () => {
    const { data, total } = await dbSource.fetchPlaces({ limit: 100 });
    expect(total).toBe(SEEDED);
    expect(data.length).toBe(SEEDED);

    expect(data[0].id).toBe(AUDIO_HOT_LAB);
    expect(data[1].id).toBe(GENESIS_PLAZA);

    const top = mapPlace(data[0]);
    expect(top).toMatchObject({
      id: AUDIO_HOT_LAB,
      title: "Audio Hot Lab",
      coords: "-98,-95",
      rating: 100,
      featured: false,
      creator: "SilvioDeCandia",
    });
    expect(top.image).toContain("peer-ec1.decentraland.org");

    const genesis = data.find((p) => p.id === GENESIS_PLAZA)!;
    expect(genesis.highlighted).toBe(true);
    expect(mapPlace(genesis).featured).toBe(true);
  });

  it("category filter (categories && $1) returns only the right subset", async () => {
    const { data } = await dbSource.fetchPlaces({
      categories: "music",
      limit: 100,
    });
    expect(data.length).toBeGreaterThan(0);

    const expected = entries
      .filter((e) => (e.categories ?? []).includes("music"))
      .map((e) => e.id)
      .sort();
    expect(data.map((p) => p.id).sort()).toEqual(expected);
    for (const p of data) expect(p.categories).toContain("music");
  });

  it("multi-category overlap returns the union (art OR poi)", async () => {
    const { data } = await dbSource.fetchPlaces({
      categories: ["art", "poi"],
      limit: 100,
    });
    const expected = entries
      .filter((e) =>
        (e.categories ?? []).some((c) => c === "art" || c === "poi"),
      )
      .map((e) => e.id)
      .sort();
    expect(data.map((p) => p.id).sort()).toEqual(expected);
  });

  it(">=3-char search hits FTS/ILIKE; ranks the match first", async () => {
    const { data } = await dbSource.fetchPlaces({ search: "plaza" });
    expect(data.length).toBeGreaterThan(0);
    expect(data[0].id).toBe(GENESIS_PLAZA);
    expect(
      data.every(
        (p) =>
          /plaza/i.test(p.title ?? "") || /plaza/i.test(p.description ?? ""),
      ),
    ).toBe(true);
  });

  it("a 1-2 char search short-circuits to empty (matches find_list)", async () => {
    const res = await dbSource.fetchPlaces({ search: "ab" });
    expect(res).toEqual({ data: [], total: 0 });
  });

  it("fetchPlace(id) returns the seeded row", async () => {
    const place = await dbSource.fetchPlace(GENESIS_PLAZA);
    expect(place).not.toBeNull();
    if (!place) return;
    expect(place.id).toBe(GENESIS_PLAZA);
    expect(place.title).toBe("Genesis Plaza");
    expect(place.base_position).toBe("-3,-2");
    expect(Array.isArray(place.positions)).toBe(true);
    expect(place.positions).toContain("-3,-2");
  });

  it("fetchPlace(unknown) throws 404 CatalystError", async () => {
    await expect(dbSource.fetchPlace("does-not-exist")).rejects.toMatchObject({
      status: 404,
    });
  });

  it("fetchCategories returns counts with i18n labels", async () => {
    const cats = await dbSource.fetchCategories();
    expect(cats.length).toBeGreaterThan(0);

    const expected = new Map<string, number>();
    for (const e of entries)
      for (const c of e.categories ?? [])
        expected.set(c, (expected.get(c) ?? 0) + 1);

    const got = new Map(cats.map((c) => [c.name, c.count]));
    for (const [name, count] of expected)
      expect(got.get(name)).toBe(count);

    for (let i = 1; i < cats.length; i++)
      expect(cats[i - 1].count).toBeGreaterThanOrEqual(cats[i].count);

    const music = cats.find((c) => c.name === "music");
    expect(music?.i18n?.en).toBe("🎵 Music");
  });

  it("places.server.loadPlaces takes the DB path (CATALYST_DATABASE_URL set)", async () => {
    const { data, total } = await loadPlaces({ limit: 100 });
    expect(total).toBe(SEEDED);
    expect(data.map((p) => p.id)).toContain(AUDIO_HOT_LAB);
  });

  it("places.server.loadPlace / loadCategories round-trip via DB", async () => {
    const place = await loadPlace(AUDIO_HOT_LAB);
    expect(place).not.toBeNull();
    expect(place?.title).toBe("Audio Hot Lab");

    const cats = await loadCategories();
    expect(cats.some((c) => c.name === "poi")).toBe(true);
  });
});
