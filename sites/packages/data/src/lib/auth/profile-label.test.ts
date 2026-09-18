import { describe, expect, it } from "vitest";

import { isProfileOfAddress, profileDisplayName } from "./profile-label";

const ADDRESS = "0x1234567890abcdef1234567890abcdef12345678";

describe("isProfileOfAddress", () => {
  it("accepts a profile reporting the asked address in any case or padding, via ethAddress before userId", () => {
    expect(isProfileOfAddress({ ethAddress: ADDRESS }, ADDRESS)).toBe(true);
    expect(isProfileOfAddress({ ethAddress: ADDRESS.toUpperCase() }, ADDRESS)).toBe(true);
    expect(isProfileOfAddress({ userId: ADDRESS }, ADDRESS)).toBe(true);
    expect(isProfileOfAddress({ ethAddress: ` ${ADDRESS} ` }, ADDRESS)).toBe(true);
    expect(isProfileOfAddress({ ethAddress: ADDRESS, userId: "0xdead" }, ADDRESS)).toBe(true);
  });

  it("refuses a profile reporting another address, none at all, or userId only when ethAddress differs", () => {
    expect(
      isProfileOfAddress({ ethAddress: "0x000000000000000000000000000000000000dead" }, ADDRESS),
    ).toBe(false);
    expect(isProfileOfAddress({ name: "Decentraland" }, ADDRESS)).toBe(false);
    expect(isProfileOfAddress({ ethAddress: "" }, ADDRESS)).toBe(false);
    expect(isProfileOfAddress(null, ADDRESS)).toBe(false);
    expect(isProfileOfAddress({ ethAddress: "0xdead", userId: ADDRESS }, ADDRESS)).toBe(false);
  });
});

describe("profileDisplayName", () => {
  it("shows the trimmed name, or nothing for a profile that carries none", () => {
    expect(profileDisplayName({ name: "decentraland" })).toBe("decentraland");
    expect(profileDisplayName({ name: "  Decentraland  " })).toBe("Decentraland");
    expect(profileDisplayName({ name: "   " })).toBe("");
    expect(profileDisplayName(null)).toBe("");
  });

  it("escapes a reordering control and cuts a name long enough to push the screen aside", () => {
    const shown = profileDisplayName({ name: "Deco\u{202e}dnal" });
    expect(shown).not.toContain("\u{202e}");
    expect(shown).toContain("\\u{202e}");
    const long = profileDisplayName({ name: "n".repeat(200) });
    expect(long.length).toBe(40);
    expect(long.endsWith("\u{2026}")).toBe(true);
  });
});
