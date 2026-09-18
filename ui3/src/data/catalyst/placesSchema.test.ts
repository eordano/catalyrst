import { describe, expect, test, vi } from "vitest";

import { fetchPlaces } from "./placesSchema";

const summaryRow = {
  id: "9270ae0d-b0f2-4c29-a9b6-77ef136f1d4c",
  title: "Winterfest Hockey 2026",
  image:
    "https://peer-ec1.decentraland.org/content/contents/bafybeidzo4ykewzxinkrher4km724igr6xb2gqm5ostpgv5e3gbnjkwnam",
  base_position: "-150,99",
  positions: ["-150,99", "-150,100"],
  contact_name: "PepeGawd",
  categories: ["game", "sports"],
  user_count: 9,
  user_visits: 1204,
  favorites: 31,
  likes: 27,
  like_rate: 1.0,
  highlighted: false,
  world: false,
  world_name: null,
};

function jsonResponse(body: unknown): Promise<Response> {
  return Promise.resolve(
    new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
  );
}

describe("fetchPlaces", () => {
  test("one GET to the places summary API yields fully-populated picker cards", async () => {
    const fetchImpl = vi.fn((_input: RequestInfo | URL) =>
      jsonResponse({ ok: true, data: [summaryRow], total: 1 }),
    );
    const out = await fetchPlaces(
      { limit: 48, order_by: "most_active", order: "desc" },
      { fetchImpl: fetchImpl as unknown as typeof fetch },
    );

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    const url = String(fetchImpl.mock.calls[0]?.[0]);
    expect(url).toContain("/api/places?");
    expect(url).toContain("limit=48");
    expect(url).toContain("order_by=most_active");
    expect(url).not.toContain("entities");

    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({
      id: summaryRow.id,
      title: "Winterfest Hockey 2026",
      coords: "-150,99",
      x: -150,
      y: 99,
      players: 9,
      live: true,
      rating: 100,
      creator: "PepeGawd",
      world: false,
    });
    expect(out[0]?.image).toContain("/content/contents/");
  });

  test("bad rows and rows without coordinates are dropped, and an unknown live count stays unknown", async () => {
    const { base_position: _dropped, ...noCoords } = summaryRow;
    const fetchImpl = vi.fn(() =>
      jsonResponse({
        ok: true,
        data: [{ nope: true }, noCoords, { ...summaryRow, user_count: null }, summaryRow],
        total: 4,
      }),
    );
    const out = await fetchPlaces({}, { fetchImpl: fetchImpl as unknown as typeof fetch });
    expect(out).toHaveLength(2);
    expect(out[0]?.players).toBeNull();
    expect(out[0]?.live).toBe(false);
    expect(out[1]?.players).toBe(9);
  });
});
