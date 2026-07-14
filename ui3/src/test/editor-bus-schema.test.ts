import { describe, expect, test } from "vitest";
import { SceneToPageMessageSchema } from "../generated/editor-bus-schemas";

// The editor scene is a separate build in another language, and both receivers
// of its messages (src/editor/editor-bus.ts and src/editor/mcp-bridge.ts) used
// to take `env.msg` as a cast behind one coarse guard. Past that guard a
// renamed or wrong-typed field was not an error: editor-bus.ts fanned it out to
// listeners and mcp-bridge.ts relayed it to the MCP server verbatim.
//
// So every case below asserts BOTH halves -- that the schema rejects it and
// that the old guard let it through. A case the old guard already caught would
// prove nothing about what the schema added.

// The guard exactly as it shipped, applied to the envelope the scene actually
// posts. It inspects the envelope and never the message body, which is why a
// drifted payload reached the consumers untouched.
const oldGuard = (msg: unknown) => {
  const env = { to: "page", msg } as { to?: unknown; msg?: unknown } | null;
  return !(!env || typeof env !== "object" || env.to !== "page" || !env.msg);
};

const sceneReady = {
  type: "scene-ready",
  bridge: 8,
  scene: {
    hash: "bafk",
    title: "Genesis Plaza",
    parcels: [{ x: 0, y: 0 }],
    isPortable: false,
    isBroken: false,
    isBlocked: false,
    isSuper: false,
    sdkVersion: "7",
  },
  frozen: false,
  tool: "select",
  orientGlobal: false,
  pivotEach: false,
  selected: [],
  active: null,
};

describe("editor bus scene-to-page validation", () => {
  const cases: [string, unknown, boolean][] = [
    ["valid scene-ready", sceneReady, true],
    ["valid rpc-reply", { type: "rpc-reply", id: "t-1", ok: true, result: 3 }, true],
    // LiveSceneInfo is filled by the engine's own JS API rather than by the
    // ts-rs type, so it is the field most able to drift out from under us.
    [
      "scene-ready missing sdkVersion",
      { ...sceneReady, scene: { ...sceneReady.scene, sdkVersion: undefined } },
      false,
    ],
    [
      "scene-ready parcels as tuples instead of {x,y}",
      { ...sceneReady, scene: { ...sceneReady.scene, parcels: [[0, 0]] } },
      false,
    ],
    ["rpc-reply with ok renamed to success", { type: "rpc-reply", id: "t-1", success: true }, false],
    ["unknown message type", { type: "brand-new", payload: 1 }, false],
    ["selection with selected as a string", { type: "selection", selected: "a", active: null }, false],
  ];
  for (const [name, value, shouldPass] of cases) {
    test(name, () => {
      expect(SceneToPageMessageSchema.safeParse(value).success).toBe(shouldPass);
      // Every one of these cleared the guard that shipped before the schema.
      expect(oldGuard(value)).toBe(true);
    });
  }

  // Extra keys are ignored on purpose: the engine adding a field must not take
  // the editor down. Asserted so a later `.strict()` cannot slip in unnoticed.
  test("tolerates unknown extra keys", () => {
    expect(SceneToPageMessageSchema.safeParse({ ...sceneReady, futureField: 1 }).success).toBe(true);
  });
});
