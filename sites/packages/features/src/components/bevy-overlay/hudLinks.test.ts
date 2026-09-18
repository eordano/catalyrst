import { describe, expect, it } from "vitest";

import { hudLinkFor, hudLinkPath } from "./hudLinks";

const ADDR = "0x92de52247aeae00fcfb18072c8564f3549b64f9c";

describe("hudLinkFor", () => {
  it("maps the sidebar's story links onto the overlay routes, threading the address where the route reads it", () => {
    expect(hudLinkFor("Explorer/Pages/Marketplace", null)).toBe("/bevy-overlay/explore?tab=marketplace");
    expect(hudLinkFor("Explorer/Pages/Help", ADDR)).toBe("/support");
    expect(hudLinkFor("Explorer/Pages/Backpack", null)).toBe("/bevy-overlay/backpack-equip");
    expect(hudLinkFor("Explorer/Pages/Backpack", ADDR)).toBe(`/bevy-overlay/backpack-equip?address=${ADDR}`);
    expect(hudLinkFor("Explorer/Pages/Passport", ADDR)).toBe(`/bevy-overlay/passport?address=${ADDR}`);
    expect(hudLinkFor("Explorer/Pages/Settings", ADDR)).toBe("/bevy-overlay/settings");
  });

  it("leaves unknown, in-HUD and missing links inert", () => {
    expect(hudLinkFor("Explorer/Components/VoiceChat", null)).toBeNull();
    expect(hudLinkFor("Explorer/Frames/Chat", null)).toBeNull();
    expect(hudLinkFor(null, ADDR)).toBeNull();
    expect(hudLinkFor(undefined, ADDR)).toBeNull();
  });
});

describe("hudLinkPath", () => {
  function target(chain: (string | null)[]) {
    const nodes = chain.map((linkto) => ({
      getAttribute: (name: string) => (name === "data-sb-linkto" ? linkto : null),
    }));
    return {
      closest: () => nodes.find((n) => n.getAttribute("data-sb-linkto") != null) ?? null,
    } as unknown as EventTarget;
  }

  it("resolves from the closest linked ancestor and ignores targets without one", () => {
    expect(hudLinkPath(target([null, "Explorer/Pages/Marketplace"]), null)).toBe("/bevy-overlay/explore?tab=marketplace");
    expect(hudLinkPath(target([null]), null)).toBeNull();
    expect(hudLinkPath(null, null)).toBeNull();
    expect(hudLinkPath({} as EventTarget, null)).toBeNull();
  });
});
