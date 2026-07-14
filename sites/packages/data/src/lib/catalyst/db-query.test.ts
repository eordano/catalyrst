import { describe, it, expect } from "vitest";

import {
  buildWhere,
  buildListQuery,
  buildCountQuery,
} from "./db-query";
import { placeFromDbRow } from "./db-row";
import { mapPlace } from "./places/index";

describe("db places query builders (faithful to catalyrst-places SQL)", () => {
  it("no filters: disabled-only WHERE, like_score order, default paging", () => {
    const q = buildListQuery({});
    expect(q.text).toContain("WHERE disabled IS FALSE");
    expect(q.text).toContain(
      "ORDER BY NULLIF(raw->>'like_score','')::float8 DESC NULLS LAST, deployed_at DESC",
    );
    expect(q.text).toContain("LIMIT 100 OFFSET 0");
    expect(q.values).toEqual([]);
  });

  it("categories -> array-overlap bind ($1), string or list", () => {
    expect(buildWhere({ categories: "music" }).clause).toContain(
      "categories && $1",
    );
    expect(buildWhere({ categories: "music" }).values).toEqual([["music"]]);
    expect(buildWhere({ categories: ["music", "art"] }).values).toEqual([
      ["music", "art"],
    ]);
  });

  it("search >=3 chars -> FTS+ILIKE clause + rank prefix; term bound 3x", () => {
    const q = buildListQuery({ search: "plaza" });
    expect(q.text).toContain("plainto_tsquery('english', $1)");
    expect(q.text).toContain("title ILIKE $2 OR description ILIKE $2");
    expect(q.text).toContain("ts_rank_cd(");
    expect(q.values).toEqual(["plaza", "%plaza%", "plaza"]);
  });

  it("limit clamps to 0..100, offset floors", () => {
    expect(buildListQuery({ limit: 999 }).text).toContain("LIMIT 100");
    expect(buildListQuery({ limit: -5 }).text).toContain("LIMIT 0");
    expect(buildListQuery({ offset: 40 }).text).toContain("OFFSET 40");
  });

  it("count query mirrors the WHERE without paging/order", () => {
    const q = buildCountQuery({ categories: "art" });
    expect(q.text).toContain(
      "SELECT count(*)::bigint AS total FROM place WHERE disabled IS FALSE AND categories && $1",
    );
    expect(q.values).toEqual([["art"]]);
  });
});

describe("placeFromDbRow + mapPlace on a Postgres-style row", () => {
  it("normalizes a row (nulls + extra cols) into a card", () => {
    const row = {
      id: "abc",
      title: "Genesis Plaza",
      description: null,
      image: "https://x/i.png",
      owner: null,
      creator_address: "0x1",
      positions: ["0,0"],
      base_position: "0,0",
      contact_name: "DAO",
      content_rating: "T",
      disabled: false,
      deployed_at: new Date(),
      favorites: 10,
      likes: 5,
      dislikes: 1,
      categories: ["social"],
      tags: [],
      highlighted: true,
      highlighted_image: null,
      world: false,
      world_name: null,
      user_count: 3,
      user_visits: 100,
      like_rate: 0.834,
      like_score: 0.7,
    };
    const place = placeFromDbRow(row);
    expect(place.user_count).toBe(3);
    expect("content_rating" in place).toBe(false);
    expect("like_score" in place).toBe(false);
    expect(mapPlace(place)).toMatchObject({
      title: "Genesis Plaza",
      players: 3,
      live: true,
      rating: 83,
      coords: "0,0",
      featured: true,
      creator: "DAO",
    });
  });

  it("null user_count -> players 0 / not live; null title -> empty", () => {
    const card = mapPlace(
      placeFromDbRow({
        id: "x",
        base_position: "1,1",
        user_count: null,
        like_rate: null,
      }),
    );
    expect(card.players).toBe(0);
    expect(card.live).toBe(false);
    expect(card.title).toBe("");
    expect(card.rating).toBe(0);
  });
});
