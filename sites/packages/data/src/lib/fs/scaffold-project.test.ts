import { describe, expect, it, vi } from "vitest";

import {
  buildScaffoldFiles,
  writeScaffoldFiles,
  projectSlug,
  SCENE_JSON_FILENAME,
  COMPOSITE_FILENAME,
} from "./scaffold-project";
import {
  parseComposite,
  entityName,
  listEntities,
  getComponentValue,
} from "../catalyst/creator-hub/scene-composite";
import {
  TEMPLATE_COMPOSITE_IDS,
  buildTemplateComposite,
} from "./template-composites";

function fileMap(files: { path: string; text: string }[]): Record<string, string> {
  return Object.fromEntries(files.map((f) => [f.path, f.text]));
}

describe("buildScaffoldFiles \u{2014} scene.json carries the typed name + layout", () => {
  it("writes the USER-TYPED name into scene.json display.title", () => {
    const files = buildScaffoldFiles({ name: "My Tavern", template: "empty" });
    const sj = JSON.parse(fileMap(files)[SCENE_JSON_FILENAME]);
    expect(sj.display.title).toBe("My Tavern");
    expect(sj.ecs7).toBe(true);
    expect(sj.runtimeVersion).toBe("7");
    expect(sj.main).toBe("bin/index.js");
  });

  it("expands the parcel layout into scene.parcels + base", () => {
    const files = buildScaffoldFiles({ name: "Grid", template: "empty", layout: "2x2" });
    const sj = JSON.parse(fileMap(files)[SCENE_JSON_FILENAME]);
    expect(sj.scene.parcels).toEqual(["0,0", "1,0", "0,1", "1,1"]);
    expect(sj.scene.base).toBe("0,0");
  });

  it("honors an explicit parcel list", () => {
    const files = buildScaffoldFiles({ name: "P", parcels: ["10,20", "11,20"] });
    const sj = JSON.parse(fileMap(files)[SCENE_JSON_FILENAME]);
    expect(sj.scene.parcels).toEqual(["10,20", "11,20"]);
    expect(sj.scene.base).toBe("10,20");
  });

  it("falls back to a default name when none is typed", () => {
    const files = buildScaffoldFiles({});
    const sj = JSON.parse(fileMap(files)[SCENE_JSON_FILENAME]);
    expect(sj.display.title).toBe("My Awesome Scene");
  });

  it("tsconfig extends the ecs7 preset @dcl/sdk ACTUALLY ships", () => {
    const files = buildScaffoldFiles({ name: "T", template: "tower-defense" });
    const ts = JSON.parse(fileMap(files)["tsconfig.json"]);
    expect(ts.extends).toBe("@dcl/sdk/types/tsconfig.ecs7.json");
  });
});

describe("buildScaffoldFiles \u{2014} main.composite reflects the template/seed", () => {
  it("an empty template yields a valid empty composite (root only)", () => {
    const files = buildScaffoldFiles({ name: "Blank", template: "empty" });
    const comp = parseComposite(JSON.parse(fileMap(files)[COMPOSITE_FILENAME]));
    expect(comp.version).toBe(1);
    expect(listEntities(comp).filter((id) => id >= 512)).toEqual([]);
  });

  it("an UNKNOWN non-empty template falls back to the spawn-point-only seed", () => {
    const files = buildScaffoldFiles({ name: "Tower", template: "some-future-template" });
    const comp = parseComposite(JSON.parse(fileMap(files)[COMPOSITE_FILENAME]));
    const authored = listEntities(comp).filter((id) => id >= 512);
    expect(authored).toEqual([512]);
    expect(entityName(comp, 512)).toBe("Spawn Point");
  });

  it("a starter template seeds its REAL curated scene content (entities + catalog GLBs)", () => {
    const files = buildScaffoldFiles({ name: "Tower", template: "tower-defense" });
    const comp = parseComposite(JSON.parse(fileMap(files)[COMPOSITE_FILENAME]));
    const authored = listEntities(comp).filter((id) => id >= 512);
    expect(authored.length).toBeGreaterThanOrEqual(8);
    expect(entityName(comp, 512)).toBe("Spawn Point");
    const names = authored.map((id) => entityName(comp, id));
    expect(names).toContain("Spawn Gate");
    expect(names).toContain("Creep Spider A");
    const gltfBlock = comp.components.find((b) => b.name === "core::GltfContainer");
    expect(gltfBlock).toBeDefined();
    for (const env of Object.values(gltfBlock!.data)) {
      expect((env.json as { src: string }).src).toMatch(
        /^assets\/imported\/template-assets\/Qm[a-zA-Z0-9]+\.glb$/,
      );
    }
  });

  it("EVERY starter template ships >= 8 authored entities, a Spawn Point and valid transforms", () => {
    expect(TEMPLATE_COMPOSITE_IDS.sort()).toEqual(
      [
        "castaway-2048",
        "escape-room",
        "memory-game",
        "nft-art-wall",
        "tower-defense",
      ].sort(),
    );
    for (const id of TEMPLATE_COMPOSITE_IDS) {
      const comp = buildTemplateComposite(id)!;
      const authored = listEntities(comp).filter((e) => e >= 512);
      expect(authored.length, id).toBeGreaterThanOrEqual(8);
      expect(entityName(comp, 512), id).toBe("Spawn Point");
      for (const eid of authored) {
        const t = getComponentValue(comp, eid, "core::Transform") as {
          position: { x: number; y: number; z: number };
          scale: { x: number; y: number; z: number };
          rotation: { w: number };
          parent: number;
        };
        expect(t, `${id}#${eid} transform`).toBeDefined();
        expect(t.position.x, `${id}#${eid} x`).toBeGreaterThanOrEqual(0);
        expect(t.position.x, `${id}#${eid} x`).toBeLessThanOrEqual(16);
        expect(t.position.z, `${id}#${eid} z`).toBeGreaterThanOrEqual(0);
        expect(t.position.z, `${id}#${eid} z`).toBeLessThanOrEqual(16);
        expect(t.scale.x, `${id}#${eid} scale`).toBeGreaterThan(0);
        expect(
          t.parent === 0 || authored.includes(t.parent),
          `${id}#${eid} parent`,
        ).toBe(true);
        expect(entityName(comp, eid), `${id}#${eid} name`).not.toMatch(/^Entity \d+$/);
      }
    }
  });

  it("starter templates ship a themed SDK7 starter index.ts with an honest header", () => {
    const files = buildScaffoldFiles({ name: "Def", template: "tower-defense" });
    const map = fileMap(files);
    expect(map["src/index.ts"]).toContain("Creep Spider");
    expect(map["src/index.ts"]).toContain("NOT a port");
    expect(map["src/index.ts"]).toContain("@dcl/sdk/ecs");
    expect(map["README.md"]).toContain("What this scaffold contains");
    const empty = fileMap(buildScaffoldFiles({ name: "E", template: "empty" }));
    expect(empty["src/index.ts"]).toContain("scaffolded by the Decentraland Creator Hub");
    expect(empty["README.md"]).not.toContain("What this scaffold contains");
  });
});

describe("buildScaffoldFiles \u{2014} template recorded in the scaffold", () => {
  it("records the chosen template id + github scene in README and scene.json", () => {
    const files = buildScaffoldFiles({
      name: "Defense",
      template: "tower-defense",
      templateTitle: "Tower Defense",
      githubLink: "https://github.com/decentraland-scenes/Tower-defense",
    });
    const map = fileMap(files);
    expect(map["README.md"]).toContain("tower-defense");
    expect(map["README.md"]).toContain(
      "https://github.com/decentraland-scenes/Tower-defense",
    );
    const sj = JSON.parse(map[SCENE_JSON_FILENAME]);
    expect(sj.tags).toContain("tower-defense");
    expect(sj.display.description).toContain("Tower Defense");
  });
});

describe("projectSlug", () => {
  it("slugifies a name into a folder-safe string", () => {
    expect(projectSlug("My Tavern!")).toBe("my-tavern");
    expect(projectSlug("")).toBe("new-scene");
  });
});

describe("writeScaffoldFiles \u{2014} real disk write", () => {
  it("writes every file in place via an injected directory handle", async () => {
    const written: Record<string, string> = {};

    const makeDir = (prefix: string, dirName = ""): unknown => ({
      name: dirName,
      getDirectoryHandle: async (name: string) => makeDir(`${prefix}${name}/`, name),
      getFileHandle: async (name: string) => ({
        createWritable: async () => {
          let buf = "";
          return {
            write: async (d: string) => {
              buf += d;
            },
            close: async () => {
              written[`${prefix}${name}`] = buf;
            },
          };
        },
      }),
    });

    const files = buildScaffoldFiles({ name: "Disk Scene", template: "empty" });
    const res = await writeScaffoldFiles(files, {
      name: "Disk Scene",
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      dir: makeDir("") as any,
    });

    expect(res.written).toBe(true);
    expect(res.via).toBe("directory");
    expect(res.folder).toBe("disk-scene");
    expect((res.dir as unknown as { name: string }).name).toBe("disk-scene");

    expect(written[`disk-scene/${SCENE_JSON_FILENAME}`]).toBeDefined();
    expect(written["disk-scene/src/index.ts"]).toBeDefined();
    const sj = JSON.parse(written[`disk-scene/${SCENE_JSON_FILENAME}`]);
    expect(sj.display.title).toBe("Disk Scene");
    const comp = parseComposite(JSON.parse(written[`disk-scene/${COMPOSITE_FILENAME}`]));
    expect(comp.version).toBe(1);
  });

  it("falls back to a per-file download writer when forced", async () => {
    const downloads: Record<string, string> = {};
    const downloadWriter = vi.fn(async (name: string, text: string) => {
      downloads[name] = text;
      return "downloaded" as const;
    });

    const files = buildScaffoldFiles({ name: "DL", template: "empty" });
    const res = await writeScaffoldFiles(files, {
      name: "DL",
      forceDownload: true,
      downloadWriter,
    });

    expect(res.written).toBe(true);
    expect(res.via).toBe("download");
    expect(downloads["src-index.ts"]).toBeDefined();
    expect(downloads[SCENE_JSON_FILENAME]).toBeDefined();
  });
});
