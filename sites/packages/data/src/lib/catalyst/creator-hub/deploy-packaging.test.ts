import { describe, expect, it } from "vitest";

import {
  buildDraftDeployFiles,
  packageSceneAssets,
  sceneMainOf,
  EMPTY_SRC_ERROR,
  SEEDED_ASSET_DIR,
} from "./deploy-packaging";

const CID = "QmWM8PmLyebMayuY3YkKLX9mZoAAWb9kk69hueAyTwfkvq";

const GLB_BYTES = new Uint8Array([1, 2, 3, 4]);
const RUNTIME_BYTES = new TextEncoder().encode("game-runtime");

function stubFetch(handlers: Record<string, Uint8Array>): typeof fetch {
  return (async (input: RequestInfo | URL) => {
    const url = String(input);
    for (const [prefix, bytes] of Object.entries(handlers)) {
      if (url.startsWith(prefix)) {
        return new Response(bytes.slice().buffer as ArrayBuffer, { status: 200 });
      }
    }
    return new Response(null, { status: 404 });
  }) as typeof fetch;
}

function compositeWith(srcs: string[]): string {
  return JSON.stringify({
    version: 1,
    components: [
      {
        name: "core::GltfContainer",
        data: Object.fromEntries(srcs.map((src, i) => [String(513 + i), { json: { src } }])),
      },
    ],
  });
}

describe("packageSceneAssets", () => {
  it("rewrites absolute builder srcs to seeded relative paths and fetches seeded srcs missing from disk", async () => {
    const seeded = `${SEEDED_ASSET_DIR}/${CID}.glb`;
    const absolute = await packageSceneAssets({
      compositeText: compositeWith([`https://catalyst.example.com/builder-items/${CID}`]),
      fileKeys: ["scene.json"],
      fetchImpl: stubFetch({ "/builder-items/": GLB_BYTES }),
    });
    expect(absolute.changed).toBe(true);
    expect(absolute.missing).toEqual([]);
    expect(absolute.extra.map((f) => f.file)).toEqual([seeded]);
    expect(absolute.compositeText).toContain(seeded);
    expect(absolute.compositeText).not.toContain("https://");

    const relative = await packageSceneAssets({
      compositeText: compositeWith([seeded]),
      fileKeys: ["scene.json"],
      fetchImpl: stubFetch({ "/builder-items/": GLB_BYTES }),
    });
    expect(relative.changed).toBe(false);
    expect(relative.missing).toEqual([]);
    expect(relative.extra.map((f) => f.file)).toEqual([seeded]);
  });

  it("leaves present files alone and flags unknown relative srcs and unfetchable builder assets as missing", async () => {
    const seeded = `${SEEDED_ASSET_DIR}/${CID}.glb`;
    const present = await packageSceneAssets({
      compositeText: compositeWith([seeded, "models/custom.glb"]),
      fileKeys: ["scene.json", seeded],
      fetchImpl: stubFetch({}),
    });
    expect(present.extra).toEqual([]);
    expect(present.missing).toEqual(["models/custom.glb"]);

    const unfetchable = await packageSceneAssets({
      compositeText: compositeWith([`/builder-items/${CID}`]),
      fileKeys: [],
      fetchImpl: stubFetch({}),
    });
    expect(unfetchable.changed).toBe(false);
    expect(unfetchable.missing).toEqual([`/builder-items/${CID}`]);
  });

  it("resolves imported catalog items through the asset-packs catalog, flags unknown ones, and skips ones already on disk", async () => {
    const itemId = "a2f47727-3f6c-4313-ae76-8034fafa2e5b";
    const src = `assets/imported/${itemId}/pebbles.glb`;
    const packs = new TextEncoder().encode(
      JSON.stringify({
        data: [
          {
            assets: [
              { id: itemId, contents: { "pebbles.glb": CID, "thumbnail.png": `${CID}t` } },
            ],
          },
        ],
      }),
    );
    const resolved = await packageSceneAssets({
      compositeText: compositeWith([src]),
      fileKeys: ["scene.json"],
      fetchImpl: stubFetch({ "/builder-api/v1/assetPacks": packs, "/builder-items/": GLB_BYTES }),
    });
    expect(resolved.changed).toBe(false);
    expect(resolved.missing).toEqual([]);
    expect(resolved.extra.map((f) => f.file).sort()).toEqual([
      src,
      `assets/imported/${itemId}/thumbnail.png`,
    ]);

    const unknown = await packageSceneAssets({
      compositeText: compositeWith([src]),
      fileKeys: ["scene.json"],
      fetchImpl: stubFetch({
        "/builder-api/v1/assetPacks": new TextEncoder().encode('{"data":[]}'),
        "/builder-items/": GLB_BYTES,
      }),
    });
    expect(unknown.missing).toEqual([src]);

    const onDisk = await packageSceneAssets({
      compositeText: compositeWith([src]),
      fileKeys: ["scene.json", src],
      fetchImpl: stubFetch({}),
    });
    expect(onDisk.extra).toEqual([]);
    expect(onDisk.missing).toEqual([]);
  });

  it("counts empty GltfContainer srcs so the deploy gate can reject stripped composites, zero for a healthy one", async () => {
    const stripped = await packageSceneAssets({
      compositeText: compositeWith(["", "  ", "models/real.glb"]),
      fileKeys: ["scene.json", "models/real.glb"],
      fetchImpl: stubFetch({}),
    });
    expect(stripped.emptySrc).toBe(2);
    expect(stripped.missing).toEqual([]);
    expect(EMPTY_SRC_ERROR(2)).toContain("2 placed items");
    expect(EMPTY_SRC_ERROR(2)).toContain("empty GltfContainer src");
    const healthy = await packageSceneAssets({
      compositeText: compositeWith(["models/real.glb"]),
      fileKeys: ["models/real.glb"],
      fetchImpl: stubFetch({}),
    });
    expect(healthy.emptySrc).toBe(0);
  });
});

describe("buildDraftDeployFiles", () => {
  const fetchImpl = stubFetch({
    "/builder-items/": GLB_BYTES,
    "/template-bundles/games.js": RUNTIME_BYTES,
  });

  it("packages a template draft with composite, code edits, assets and the game runtime; sceneMainOf falls back to bin/index.js", async () => {
    const pack = await buildDraftDeployFiles(
      {
        title: "My Tower",
        base: "0,0",
        template: "tower-defense",
        composite: compositeWith([`/builder-items/${CID}`]),
        codeFiles: { "src/index.ts": "export function main() {}\n" },
      },
      { fetchImpl },
    );
    expect(pack.missing).toEqual([]);
    expect(pack.runtimeInjected).toBe(true);
    const names = pack.files.map((f) => f.file);
    expect(names).toContain("scene.json");
    expect(names).toContain("main.composite");
    expect(names).toContain("src/index.ts");
    expect(names).toContain("bin/index.js");
    expect(names).toContain(`${SEEDED_ASSET_DIR}/${CID}.glb`);
    const idx = pack.files.find((f) => f.file === "src/index.ts")!;
    expect(new TextDecoder().decode(idx.content)).toBe("export function main() {}\n");
    const comp = pack.files.find((f) => f.file === "main.composite")!;
    expect(new TextDecoder().decode(comp.content)).toContain(SEEDED_ASSET_DIR);
    expect(pack.metadata.main).toBe("bin/index.js");
    const scene = pack.metadata.scene as { parcels?: string[]; base?: string };
    expect(scene.parcels).toEqual(["0,0"]);
    expect((pack.metadata.tags as string[])[0]).toBe("tower-defense");
    expect(sceneMainOf({})).toBe("bin/index.js");
    expect(sceneMainOf({ main: "bin/game.js" })).toBe("bin/game.js");
  });

  it("packages an empty draft with the idle runtime and no template tag, and throws when the runtime cannot be fetched", async () => {
    const pack = await buildDraftDeployFiles({ title: "Blank" }, { fetchImpl });
    expect(pack.runtimeInjected).toBe(true);
    expect(pack.metadata.tags).toEqual([]);
    const bin = pack.files.find((f) => f.file === "bin/index.js")!;
    expect(new TextDecoder().decode(bin.content)).toBe("game-runtime");
    await expect(
      buildDraftDeployFiles({ title: "Blank" }, { fetchImpl: stubFetch({}) }),
    ).rejects.toThrow(/runtime/i);
  });
});
