import { expect, test } from "vitest";
import { eventDestination, parcelDestination, placeDestination } from "./lobbyDestinations";
import { normalizePlace, toPlaceView } from "../../data/catalyst/places";
import { PlaceSchema } from "../../data/catalyst/placesSchema";
import { parseEvent } from "../../data/catalyst/events";

test("parcel targets reject missing and malformed coordinates rather than jumping to 0,0", () => {
  expect(parcelDestination("Plaza", "10,-20")).toEqual({ name: "Plaza", x: 10, y: -20 });
  for (const coords of ["", "NaN,1", "1", "1,2,3", "1.4,0"]) expect(parcelDestination("Missing", coords)).toBeNull();
});

test("world cards require an actual world destination", () => {
  const place = toPlaceView(normalizePlace(PlaceSchema.parse({ id: "w", title: "World", world: true, base_position: "0,0", positions: [], categories: [], user_visits: 0, favorites: 0, likes: 0, highlighted: true })));
  expect(placeDestination(place)).toBeNull();
  expect(placeDestination({ ...place, worldName: "hello.dcl.eth" })).toEqual({ name: "World", realm: "hello.dcl.eth" });
});

test("live events without coordinates cannot invent a destination", () => {
  const event = parseEvent({ id: "event", name: "Party", all_day: false, position: [], coordinates: [], live: true, highlighted: false, trending: false, recurrent: false, total_attendees: 0, world: false })!;
  expect(eventDestination(event)).toBeNull();
  expect(eventDestination({ ...event, x: -5, y: 2 })).toEqual({ name: "Party", x: -5, y: 2 });
});
