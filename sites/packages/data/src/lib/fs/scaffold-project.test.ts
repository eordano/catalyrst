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

function sceneJson(input: Parameters<typeof buildScaffoldFiles>[0]) {
  return JSON.parse(fileMap(buildScaffoldFiles(input))[SCENE_JSON_FILENAME]);
}

describe("buildScaffoldFiles \u{2014} scene.json carries the typed name + layout", () => {
  it("writes the USER-TYPED name (or the default) into display.title, expands a layout or honors an explicit parcel list, and extends the ecs7 preset @dcl/sdk ACTUALLY ships", () => {
    const typed = sceneJson({ name: "My Tavern", template: "empty" });
    expect(typed.display.title).toBe("My Tavern");
    expect(typed.ecs7).toBe(true);
    expect(typed.runtimeVersion).toBe("7");
    expect(typed.main).toBe("bin/index.js");
    expect(sceneJson({}).display.title).toBe("My Awesome Scene");

    const grid = sceneJson({ name: "Grid", template: "empty", layout: "2x2" });
    expect(grid.scene.parcels).toEqual(["0,0", "1,0", "0,1", "1,1"]);
    expect(grid.scene.base).toBe("0,0");
    const explicit = sceneJson({ name: "P", parcels: ["10,20", "11,20"] });
    expect(explicit.scene.parcels).toEqual(["10,20", "11,20"]);
    expect(explicit.scene.base).toBe("10,20");

    const ts = JSON.parse(
      fileMap(buildScaffoldFiles({ name: "T", template: "tower-defense" }))["tsconfig.json"],
    );
    expect(ts.extends).toBe("@dcl/sdk/types/tsconfig.ecs7.json");

    expect(projectSlug("My Tavern!")).toBe("my-tavern");
    expect(projectSlug("")).toBe("new-scene");
  });
});

describe("buildScaffoldFiles \u{2014} main.composite reflects the template/seed", () => {
  it("empty yields a root-only composite, an UNKNOWN template falls back to the spawn-point seed, and a starter seeds its REAL curated content with catalog GLBs", () => {
    const empty = parseComposite(
      JSON.parse(fileMap(buildScaffoldFiles({ name: "Blank", template: "empty" }))[COMPOSITE_FILENAME]),
    );
    expect(empty.version).toBe(1);
    expect(listEntities(empty).filter((id) => id >= 512)).toEqual([]);

    const unknown = parseComposite(
      JSON.parse(
        fileMap(buildScaffoldFiles({ name: "Tower", template: "some-future-template" }))[COMPOSITE_FILENAME],
      ),
    );
    expect(listEntities(unknown).filter((id) => id >= 512)).toEqual([512]);
    expect(entityName(unknown, 512)).toBe("Spawn Point");

    const comp = parseComposite(
      JSON.parse(fileMap(buildScaffoldFiles({ name: "Tower", template: "tower-defense" }))[COMPOSITE_FILENAME]),
    );
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
    expect(TEMPLATE_COMPOSITE_IDS.length).toBeGreaterThan(0);
    const offenders: string[] = [];
    for (const id of TEMPLATE_COMPOSITE_IDS) {
      const comp = buildTemplateComposite(id)!;
      const authored = listEntities(comp).filter((e) => e >= 512);
      if (authored.length < 8) offenders.push(`${id}: ${authored.length} authored entities`);
      if (entityName(comp, 512) !== "Spawn Point") offenders.push(`${id}: no Spawn Point at 512`);
      for (const eid of authored) {
        const t = getComponentValue(comp, eid, "core::Transform") as
          | {
              position: { x: number; y: number; z: number };
              scale: { x: number; y: number; z: number };
              rotation: { w: number };
              parent: number;
            }
          | undefined;
        if (!t) {
          offenders.push(`${id}#${eid} transform missing`);
          continue;
        }
        if (t.position.x < 0 || t.position.x > 16) offenders.push(`${id}#${eid} x`);
        if (t.position.z < 0 || t.position.z > 16) offenders.push(`${id}#${eid} z`);
        if (!(t.scale.x > 0)) offenders.push(`${id}#${eid} scale`);
        if (!(t.parent === 0 || authored.includes(t.parent))) offenders.push(`${id}#${eid} parent`);
        if (/^Entity \d+$/.test(entityName(comp, eid) ?? "")) offenders.push(`${id}#${eid} name`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it("starter templates ship a themed SDK7 index.ts with an honest header and record the template id + github scene in README and scene.json", () => {
    const map = fileMap(
      buildScaffoldFiles({
        name: "Defense",
        template: "tower-defense",
        templateTitle: "Tower Defense",
        githubLink: "https://github.com/decentraland-scenes/Tower-defense",
      }),
    );
    expect(map["src/index.ts"]).toContain("Creep Spider");
    expect(map["src/index.ts"]).toContain("NOT a port");
    expect(map["src/index.ts"]).toContain("@dcl/sdk/ecs");
    expect(map["README.md"]).toContain("tower-defense");
    expect(map["README.md"]).toContain(
      "https://github.com/decentraland-scenes/Tower-defense",
    );
    const sj = JSON.parse(map[SCENE_JSON_FILENAME]);
    expect(sj.tags).toContain("tower-defense");
    expect(sj.display.description).toContain("Tower Defense");

    const empty = fileMap(buildScaffoldFiles({ name: "E", template: "empty" }));
    expect(empty["README.md"]).not.toContain("tower-defense");
  });
});

describe("writeScaffoldFiles \u{2014} real disk write", () => {
  it("writes every file in place via an injected directory handle, and falls back to a per-file download writer when forced", async () => {
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
    expect(written["disk-scene/src/index.ts"]).toBeDefined();
    const sj = JSON.parse(written[`disk-scene/${SCENE_JSON_FILENAME}`]);
    expect(sj.display.title).toBe("Disk Scene");
    const comp = parseComposite(JSON.parse(written[`disk-scene/${COMPOSITE_FILENAME}`]));
    expect(comp.version).toBe(1);

    const downloads: Record<string, string> = {};
    const downloadWriter = vi.fn(async (name: string, text: string) => {
      downloads[name] = text;
      return "downloaded" as const;
    });
    const dl = await writeScaffoldFiles(buildScaffoldFiles({ name: "DL", template: "empty" }), {
      name: "DL",
      forceDownload: true,
      downloadWriter,
    });
    expect(dl.written).toBe(true);
    expect(dl.via).toBe("download");
    expect(downloads["src-index.ts"]).toBeDefined();
    expect(downloads[SCENE_JSON_FILENAME]).toBeDefined();
  });
});
