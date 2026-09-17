import { describe, expect, it } from "vitest";

import { isProfileOfAddress, profileDisplayName } from "./profile-label";

const ADDRESS = "0x1234567890abcdef1234567890abcdef12345678";

describe("isProfileOfAddress", () => {
  it("accepts the profile that reports the address it was asked about", () => {
    expect(isProfileOfAddress({ ethAddress: ADDRESS }, ADDRESS)).toBe(true);
    expect(isProfileOfAddress({ ethAddress: ADDRESS.toUpperCase() }, ADDRESS)).toBe(true);
    expect(isProfileOfAddress({ userId: ADDRESS }, ADDRESS)).toBe(true);
    expect(isProfileOfAddress({ ethAddress: ` ${ADDRESS} ` }, ADDRESS)).toBe(true);
  });

  it("refuses a profile that reports another address", () => {
    expect(
      isProfileOfAddress({ ethAddress: "0x000000000000000000000000000000000000dead" }, ADDRESS),
    ).toBe(false);
  });

  it("refuses a profile that reports no address at all", () => {
    expect(isProfileOfAddress({ name: "Decentraland" }, ADDRESS)).toBe(false);
    expect(isProfileOfAddress({ ethAddress: "" }, ADDRESS)).toBe(false);
    expect(isProfileOfAddress(null, ADDRESS)).toBe(false);
  });

  it("reads ethAddress before userId", () => {
    expect(isProfileOfAddress({ ethAddress: ADDRESS, userId: "0xdead" }, ADDRESS)).toBe(true);
    expect(isProfileOfAddress({ ethAddress: "0xdead", userId: ADDRESS }, ADDRESS)).toBe(false);
  });
});

describe("profileDisplayName", () => {
  it("shows the name the profile carries", () => {
    expect(profileDisplayName({ name: "decentraland" })).toBe("decentraland");
    expect(profileDisplayName({ name: "  Decentraland  " })).toBe("Decentraland");
  });

  it("shows a name that would reorder the line beside it as a visible escape", () => {
    const shown = profileDisplayName({ name: "Deco\u{202e}dnal" });
    expect(shown).not.toContain("\u{202e}");
    expect(shown).toContain("\\u{202e}");
  });

  it("cuts a name long enough to push the rest of the screen aside", () => {
    const shown = profileDisplayName({ name: "n".repeat(200) });
    expect(shown.length).toBe(40);
    expect(shown.endsWith("\u{2026}")).toBe(true);
  });

  it("has no name for a profile that carries none", () => {
    expect(profileDisplayName({ name: "   " })).toBe("");
    expect(profileDisplayName(null)).toBe("");
  });
});
