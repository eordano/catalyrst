import { describe, expect, it } from "vitest";

import { realmDeepLink, worldRealmUrl } from "./deepLink";

// These expectations are duplicated verbatim in catalyrst-types::deep_link's
// tests. The two encoders sit on opposite sides of the stack and must agree:
// a link that differs by one escape opens the client into the wrong realm
// rather than failing visibly.
describe("when building explorer deep links", () => {
  it("should form-encode the realm URL and the position separator", () => {
    expect(realmDeepLink("http://127.0.0.1:5600", "52,-68")).toBe(
      "decentraland://realm=http%3A%2F%2F127.0.0.1%3A5600&position=52%2C-68",
    );
  });

  it("should encode a path-addressed world realm", () => {
    expect(
      realmDeepLink(
        "https://realm.example.org/worlds-content-server/world/swiss-cube",
        "0,0",
      ),
    ).toBe(
      "decentraland://realm=https%3A%2F%2Frealm.example.org%2Fworlds-content-server%2Fworld%2Fswiss-cube&position=0%2C0",
    );
  });

  it("should lowercase and trim when composing a world realm URL", () => {
    expect(worldRealmUrl("https://host/worlds-content-server/", "Swiss.DCL.eth")).toBe(
      "https://host/worlds-content-server/world/swiss.dcl.eth",
    );
  });
});
