import { describe, expect, it, vi } from "vitest";

import {
  addEntity,
  buildCompositeFromHierarchy,
  listEntities,
  parseComposite,
  serializeSceneComposite,
} from "../catalyst/creator-hub/scene-composite";
import {
  normalizeEngineComposite,
  saveSceneFromEngine,
  ENGINE_GAP_SAVE_ERROR,
  type SceneHierarchyNode,
} from "./save-scene";

const SEED: SceneHierarchyNode[] = [{ entity: 0, name: "Scene", parent: 0 }];

const ENGINE_COMPOSITE = serializeSceneComposite(
  addEntity(buildCompositeFromHierarchy(SEED), { id: 512, name: "Oak Tree", parent: 0 }).composite,
);

describe("saveSceneFromEngine \u{2014} author \u{2192} save \u{2192} reopen \u{2192} persists (the fixed save path)", () => {
  it("saves the AUTHORITATIVE engine composite, not the empty seed", async () => {
    const captured: { name: string; text: string }[] = [];
    const writer = async (name: string, text: string) => {
      captured.push({ name, text });
      return "written" as const;
    };
    const exportComposite = vi.fn(async () => ENGINE_COMPOSITE);

    const res = await saveSceneFromEngine(SEED, {}, { writer, exportComposite });

    expect(exportComposite).toHaveBeenCalledTimes(1);
    expect(res.source).toBe("engine");
    expect(res.written).toBe(true);
    expect(res.entities).toBeGreaterThan(0);
    expect(captured).toHaveLength(1);

    const reopened = parseComposite(JSON.parse(captured[0].text));
    const ids = listEntities(reopened).map(String);
    expect(ids).toContain("512");
    expect(captured[0].text).toContain("Oak Tree");

    const seedText = serializeSceneComposite(buildCompositeFromHierarchy(SEED));
    expect(captured[0].text).not.toBe(seedText);
  });

  it("BLOCKS the save with an honest error when the engine gives no usable reply", async () => {
    const captured: string[] = [];
    const writer = async (_n: string, text: string) => {
      captured.push(text);
      return "written" as const;
    };
    for (const reply of [null, "", "   ", "not-json"]) {
      captured.length = 0;
      await expect(
        saveSceneFromEngine(SEED, {}, {
          writer,
          exportComposite: async () => reply,
        }),
      ).rejects.toThrow(ENGINE_GAP_SAVE_ERROR);
      expect(captured).toHaveLength(0);
    }
  });

  it("normalizeEngineComposite accepts a valid composite (string or object), rejects junk", () => {
    expect(normalizeEngineComposite(null)).toBeNull();
    expect(normalizeEngineComposite("")).toBeNull();
    expect(normalizeEngineComposite("   ")).toBeNull();
    expect(normalizeEngineComposite("not json")).toBeNull();
    expect(normalizeEngineComposite(ENGINE_COMPOSITE)).toBe(ENGINE_COMPOSITE);
    const obj = JSON.parse(ENGINE_COMPOSITE);
    expect(normalizeEngineComposite(obj)).toBe(JSON.stringify(obj));
  });
});
