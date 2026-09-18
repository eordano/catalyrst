import { afterEach, expect, test } from "vitest";
import { getRecent, pushRecent } from "./recentPlaces";
import { normalizePlace, toPlaceView } from "./catalyst/places";
import { PlaceSchema } from "./catalyst/placesSchema";

afterEach(() => localStorage.removeItem("dcl.recentPlaces"));
const place = toPlaceView(normalizePlace(PlaceSchema.parse({ id: "plaza", title: "Plaza", base_position: "10,-20", positions: ["10,-20"], categories: [], user_visits: 0, favorites: 0, likes: 0, highlighted: true, world: false })));

test("corrupt and outdated recent-visit caches do not crash the default view", () => {
  for (const data of ["{", "null", '[{"id":"old"}]']) {
    localStorage.setItem("dcl.recentPlaces", data);
    expect(getRecent()).toEqual([]);
  }
});

test("recent visits are deduplicated, newest first, and capped at 24", () => {
  for (let i = 0; i < 30; i++) pushRecent({ ...place, id: String(i) });
  pushRecent({ ...place, id: "4" });
  expect(getRecent()).toHaveLength(24);
  expect(getRecent()[0]?.id).toBe("4");
  pushRecent({ ...place, id: "4" });
  expect(getRecent().filter((item) => item.id === "4")).toHaveLength(1);
});
