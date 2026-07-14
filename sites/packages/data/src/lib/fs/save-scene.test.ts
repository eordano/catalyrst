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
  it("adds a placed entity with Transform + Name", () => {
    const c = composeEditedScene(HIERARCHY, {
      placed: { entity: 540, assetName: "Oak Tree", parent: 0 },
    });
    expect(listEntities(c)).toContain(540);
    expect(entityName(c, 540)).toBe("Oak Tree");
    expect(getComponentValue(c, 540, TRANSFORM)).toMatchObject({ parent: 0 });
  });

  it("renames a modified entity", () => {
    const c = composeEditedScene(HIERARCHY, {
      selected: { entity: 513, name: "Old Sign" },
      modifiedName: "New Sign",
    });
    expect(entityName(c, 513)).toBe("New Sign");
  });

  it("attaches a picked component to the edited entity", () => {
    const c = composeEditedScene(HIERARCHY, {
      selected: { entity: 512, name: "Floor" },
      component: "MeshCollider",
    });
    expect(getComponentValue(c, 512, "MeshCollider")).toEqual({});
  });

  it("delete removes the entity from every block", () => {
    const c = composeEditedScene(HIERARCHY, {
      selected: { entity: 513, name: "Old Sign" },
      deleted: true,
    });
    expect(listEntities(c)).not.toContain(513);
  });

  it("round-trips through serialize\u{2192}parse with the edit intact", () => {
    const c = composeEditedScene(HIERARCHY, {
      placed: { entity: 540, assetName: "Oak Tree", parent: 0 },
    });
    const text = JSON.stringify({
      version: c.version,
      components: c.components.map((b) => ({
        name: b.name,
        data: Object.fromEntries(
          Object.entries(b.data).map(([id, e]) => [id, { json: e.json }]),
        ),
      })),
    });
    const back = parseComposite(JSON.parse(text));
    expect(getComponentValue(back, 540, NAME)).toMatchObject({ value: "Oak Tree" });
  });
});

describe("saveSceneComposite \u{2014} honest write outcomes", () => {
  it("reports a real in-place write via the injected writer and returns the bytes", async () => {
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
    const back = parseComposite(JSON.parse(captured));
    expect(entityName(back, 540)).toBe("Oak Tree");
  });

  it("maps a download to written:true via:download", async () => {
    const res = await saveSceneComposite(HIERARCHY, {}, { writer: async () => "downloaded" });
    expect(res.written).toBe(true);
    expect(res.via).toBe("download");
  });

  it("does NOT claim a save when the user cancels the picker", async () => {
    const res = await saveSceneComposite(HIERARCHY, {}, { writer: async () => "canceled" });
    expect(res.written).toBe(false);
    expect(res.via).toBe("canceled");
  });
});

describe("sanitizeContentPath \u{2014} aliases filenames the local FS rejects", () => {
  it("maps U+202F/U+00A0 to plain spaces and strips FS-unsafe characters", () => {
    expect(sanitizeContentPath("models/Screenshot\u202f1.png")).toBe(
      "models/Screenshot 1.png",
    );
    expect(sanitizeContentPath("a\u00a0b.glb")).toBe("a b.glb");
    expect(sanitizeContentPath('bad<>:"|?*.glb')).toBe("bad_______.glb");
    expect(sanitizeContentPath("trailing. ")).toBe("trailing");
    expect(sanitizeContentPath("dir\u202fx/file\u202fy.png")).toBe("dir x/file y.png");
  });

  it("keeps already-safe paths byte-identical", () => {
    expect(sanitizeContentPath("models/tree.glb")).toBe("models/tree.glb");
    expect(sanitizeContentPath("a b/c d.png")).toBe("a b/c d.png");
  });
});

describe("rewriteAliasedPaths \u{2014} composite references follow the alias", () => {
  it("rewrites every string occurrence of an aliased path", () => {
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
  });

  it("returns the text unchanged for an empty alias map or unparsable input", () => {
    expect(rewriteAliasedPaths("not json", new Map([["a", "b"]]))).toBe("not json");
    expect(rewriteAliasedPaths('{"x":1}', new Map())).toBe('{"x":1}');
  });
});
