import { describe, expect, it } from "vitest";
import { mapPlace, type Place } from "./index";
import fixture from "../../../fixtures/places.json";

const first = (fixture.places.data as unknown as Place[])[0];

describe("mapPlace", () => {
  it("projects the first fixture place onto the card view-model", () => {
    const card = mapPlace(first);

    expect(card.id).toBe(first.id);
    expect(card.title).toBe(first.title);
    expect(card.players).toBe(first.user_count);
    expect(card.rating).toBe(Math.round((first.like_rate ?? 0) * 100));
    expect(card.coords).toBe(first.base_position);
    expect(card.featured).toBe(first.highlighted);
    expect(card.live).toBe((first.user_count ?? 0) > 0);
    expect(card.creator).toBe(first.contact_name ?? first.owner ?? "");
  });

  it("matches the concrete values recorded in the fixture", () => {
    const card = mapPlace(first);
    expect(card).toEqual({
      id: "830d885b-52f3-4c91-9151-9c8ec40aab63",
      title: "Audio Hot Lab",
      image:
        "https://peer-ec1.decentraland.org/content/contents/bafkreie3quzk3yvhcppse7mdupq5n63xg67iiithyxjfywjspfcaqdx4ny",
      players: 0,
      rating: 100,
      coords: "-98,-95",
      live: false,
      featured: false,
      creator: "SilvioDeCandia",
    });
  });
});
