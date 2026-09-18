import { describe, expect, it, vi } from "vitest";

import {
  composeEditedScene,
  saveSceneComposite,
  sanitizeContentPath,
  rewriteAliasedPaths,
  COMPOSITE_FILENAME,
  type SceneHierarchyNode,
} from "./save-scene";
import {
  parseComposite,
  getComponentValue,
  entityName,
  listEntities,
  TRANSFORM,
  NAME,
} from "../catalyst/creator-hub/scene-composite";
import type { SaveResult as DiskSaveResult } from "./disk";

const HIERARCHY: SceneHierarchyNode[] = [
  { entity: 0, name: "Scene", parent: 0 },
  { entity: 512, name: "Floor", parent: 0 },
  { entity: 513, name: "Old Sign", parent: 0 },
];

describe("composeEditedScene \u{2014} pure edit \u{2192} composite", () => {
  it("places an entity with Transform + Name that survives serialize\u{2192}parse, renames, attaches a picked component, and deletes from every block", () => {
    const placed = composeEditedScene(HIERARCHY, {
      placed: { entity: 540, assetName: "Oak Tree", parent: 0 },
    });
    expect(listEntities(placed)).toContain(540);
    expect(entityName(placed, 540)).toBe("Oak Tree");
    expect(getComponentValue(placed, 540, TRANSFORM)).toMatchObject({ parent: 0 });
    const text = JSON.stringify({
      version: placed.version,
      components: placed.components.map((b) => ({
        name: b.name,
        data: Object.fromEntries(
          Object.entries(b.data).map(([id, e]) => [id, { json: e.json }]),
        ),
      })),
    });
    const back = parseComposite(JSON.parse(text));
    expect(getComponentValue(back, 540, NAME)).toMatchObject({ value: "Oak Tree" });

    const renamed = composeEditedScene(HIERARCHY, {
      selected: { entity: 513, name: "Old Sign" },
      modifiedName: "New Sign",
    });
    expect(entityName(renamed, 513)).toBe("New Sign");

    const withComponent = composeEditedScene(HIERARCHY, {
      selected: { entity: 512, name: "Floor" },
      component: "MeshCollider",
    });
    expect(getComponentValue(withComponent, 512, "MeshCollider")).toEqual({});

    const deleted = composeEditedScene(HIERARCHY, {
      selected: { entity: 513, name: "Old Sign" },
      deleted: true,
    });
    expect(listEntities(deleted)).not.toContain(513);
  });
});

describe("saveSceneComposite \u{2014} honest write outcomes", () => {
  it("reports a real in-place write with the bytes, maps a download to written:true, and does NOT claim a save when the picker is canceled", async () => {
    let captured = "";
    const writer = vi.fn(
      async (_name: string, body: string): Promise<DiskSaveResult> => {
        captured = body;
        return "written";
      },
    );
    const res = await saveSceneComposite(
      HIERARCHY,
      { placed: { entity: 540, assetName: "Oak Tree", parent: 0 } },
      { writer },
    );
    expect(res.written).toBe(true);
    expect(res.via).toBe("fsa-handle");
    expect(res.filename).toBe(COMPOSITE_FILENAME);
    expect(res.text).toBe(captured);
    expect(entityName(parseComposite(JSON.parse(captured)), 540)).toBe("Oak Tree");

    const downloaded = await saveSceneComposite(HIERARCHY, {}, { writer: async () => "downloaded" });
    expect(downloaded.written).toBe(true);
    expect(downloaded.via).toBe("download");

    const canceled = await saveSceneComposite(HIERARCHY, {}, { writer: async () => "canceled" });
    expect(canceled.written).toBe(false);
    expect(canceled.via).toBe("canceled");
  });
});

describe("sanitizeContentPath \u{2014} aliases filenames the local FS rejects", () => {
  it("maps U+202F/U+00A0 to plain spaces, strips FS-unsafe characters, and keeps already-safe paths byte-identical", () => {
    expect(sanitizeContentPath("models/Screenshot\u202f1.png")).toBe(
      "models/Screenshot 1.png",
    );
    expect(sanitizeContentPath("a\u00a0b.glb")).toBe("a b.glb");
    expect(sanitizeContentPath('bad<>:"|?*.glb')).toBe("bad_______.glb");
    expect(sanitizeContentPath("trailing. ")).toBe("trailing");
    expect(sanitizeContentPath("dir\u202fx/file\u202fy.png")).toBe("dir x/file y.png");
    expect(sanitizeContentPath("models/tree.glb")).toBe("models/tree.glb");
    expect(sanitizeContentPath("a b/c d.png")).toBe("a b/c d.png");
  });
});

describe("rewriteAliasedPaths \u{2014} composite references follow the alias", () => {
  it("rewrites every string occurrence of an aliased path, and leaves the text unchanged for an empty alias map or unparsable input", () => {
    const text = JSON.stringify({
      version: 1,
      components: [
        {
          name: "core::GltfContainer",
          data: { "512": { json: { src: "models/Screenshot\u202f1.png" } } },
        },
      ],
    });
    const aliased = new Map([["models/Screenshot\u202f1.png", "models/Screenshot 1.png"]]);
    const out = JSON.parse(rewriteAliasedPaths(text, aliased));
    expect(out.components[0].data["512"].json.src).toBe("models/Screenshot 1.png");
    expect(rewriteAliasedPaths("not json", new Map([["a", "b"]]))).toBe("not json");
    expect(rewriteAliasedPaths('{"x":1}', new Map())).toBe('{"x":1}');
  });
});

it("does not persist a scene after cancellation during engine export", async () => {
  const { saveSceneFromEngine } = await import("./save-scene");
  const controller = new AbortController();
  const writer = vi.fn(async (): Promise<DiskSaveResult> => "written");
  await expect(saveSceneFromEngine([], {}, { signal: controller.signal, writer, exportComposite: async () => {
    controller.abort();
    return JSON.stringify({ version: 1, components: [] });
  } })).rejects.toMatchObject({ name: "AbortError" });
  expect(writer).not.toHaveBeenCalled();
});

it("does not start persistence for an already retired operation", async () => {
  const writer = vi.fn(async (): Promise<DiskSaveResult> => "written");
  await expect(saveSceneComposite(HIERARCHY, {}, { signal: AbortSignal.abort(), writer })).rejects.toMatchObject({ name: "AbortError" });
  expect(writer).not.toHaveBeenCalled();
});
