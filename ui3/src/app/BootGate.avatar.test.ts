import { describe, expect, test } from "vitest";

import { buildJumpInAvatarPayload } from "./BootGate";

const COLORS = {
  skinColor: { r: 0.1, g: 0.2, b: 0.3 },
  hairColor: { r: 0.4, g: 0.5, b: 0.6 },
  eyesColor: { r: 0.7, g: 0.8, b: 0.9 },
};

describe("buildJumpInAvatarPayload", () => {
  test("merges base and equip into one payload with the typed name", () => {
    const payload = buildJumpInAvatarPayload({
      name: "Alice",
      fallbackName: "RandomKoda",
      bodyShapeUrn: "urn:decentraland:off-chain:base-avatars:BaseFemale",
      ...COLORS,
      wearables: ["urn:decentraland:off-chain:base-avatars:f_sweater"],
    });
    expect(payload).toEqual({
      base: {
        bodyShapeUrn: "urn:decentraland:off-chain:base-avatars:BaseFemale",
        name: "Alice",
        ...COLORS,
      },
      equip: {
        wearableUrns: ["urn:decentraland:off-chain:base-avatars:f_sweater"],
        emoteUrns: [],
        forceRender: [],
      },
    });
  });

  test("empty typed name falls back (engine requires a non-empty base.name)", () => {
    const payload = buildJumpInAvatarPayload({
      name: "",
      fallbackName: "RandomKoda",
      bodyShapeUrn: "urn:body",
      ...COLORS,
      wearables: null,
    });
    expect(payload.base.name).toBe("RandomKoda");
    expect(payload.equip).toBeUndefined();
  });

  test("no fallback either: a random name is generated, never empty", () => {
    const payload = buildJumpInAvatarPayload({
      bodyShapeUrn: "urn:body",
      ...COLORS,
      wearables: [],
    });
    expect(typeof payload.base.name).toBe("string");
    expect((payload.base.name as string).length).toBeGreaterThan(0);
  });
});
