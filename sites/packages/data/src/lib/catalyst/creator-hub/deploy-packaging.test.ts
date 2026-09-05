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
  it("rewrites absolute builder srcs to seeded relative paths and fetches the bytes", async () => {
    const out = await packageSceneAssets({
      compositeText: compositeWith([`https://catalyst.example.com/builder-items/${CID}`]),
      fileKeys: ["scene.json"],
      fetchImpl: stubFetch({ "/builder-items/": GLB_BYTES }),
    });
    expect(out.changed).toBe(true);
    expect(out.missing).toEqual([]);
    const expected = `${SEEDED_ASSET_DIR}/${CID}.glb`;
    expect(out.extra.map((f) => f.file)).toEqual([expected]);
    expect(out.compositeText).toContain(expected);
    expect(out.compositeText).not.toContain("https://");
  });

  it("fetches seeded relative srcs that are not on disk, without rewriting", async () => {
    const src = `${SEEDED_ASSET_DIR}/${CID}.glb`;
    const out = await packageSceneAssets({
      compositeText: compositeWith([src]),
      fileKeys: ["scene.json"],
      fetchImpl: stubFetch({ "/builder-items/": GLB_BYTES }),
    });
    expect(out.changed).toBe(false);
    expect(out.missing).toEqual([]);
    expect(out.extra.map((f) => f.file)).toEqual([src]);
  });

  it("leaves present files alone and flags unknown relative srcs as missing", async () => {
    const src = `${SEEDED_ASSET_DIR}/${CID}.glb`;
    const out = await packageSceneAssets({
      compositeText: compositeWith([src, "models/custom.glb"]),
      fileKeys: ["scene.json", src],
      fetchImpl: stubFetch({}),
    });
    expect(out.extra).toEqual([]);
    expect(out.missing).toEqual(["models/custom.glb"]);
  });

  it("reports builder assets it cannot fetch as missing", async () => {
    const out = await packageSceneAssets({
      compositeText: compositeWith([`/builder-items/${CID}`]),
      fileKeys: [],
      fetchImpl: stubFetch({}),
    });
    expect(out.changed).toBe(false);
    expect(out.missing).toEqual([`/builder-items/${CID}`]);
  });

  it("resolves imported catalog items through the asset-packs catalog", async () => {
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
    const out = await packageSceneAssets({
      compositeText: compositeWith([src]),
      fileKeys: ["scene.json"],
      fetchImpl: stubFetch({
        "/builder-api/v1/assetPacks": packs,
        "/builder-items/": GLB_BYTES,
      }),
    });
    expect(out.changed).toBe(false);
    expect(out.missing).toEqual([]);
    expect(out.extra.map((f) => f.file).sort()).toEqual([
      src,
      `assets/imported/${itemId}/thumbnail.png`,
    ]);
  });

  it("flags imported items the catalog does not know as missing", async () => {
    const src = "assets/imported/a2f47727-3f6c-4313-ae76-8034fafa2e5b/pebbles.glb";
    const out = await packageSceneAssets({
      compositeText: compositeWith([src]),
      fileKeys: ["scene.json"],
      fetchImpl: stubFetch({
        "/builder-api/v1/assetPacks": new TextEncoder().encode('{"data":[]}'),
        "/builder-items/": GLB_BYTES,
      }),
    });
    expect(out.missing).toEqual([src]);
  });

  it("skips imported items already present in the project files", async () => {
    const src = "assets/imported/a2f47727-3f6c-4313-ae76-8034fafa2e5b/pebbles.glb";
    const out = await packageSceneAssets({
      compositeText: compositeWith([src]),
      fileKeys: ["scene.json", src],
      fetchImpl: stubFetch({}),
    });
    expect(out.extra).toEqual([]);
    expect(out.missing).toEqual([]);
  });

  it("counts empty GltfContainer srcs so the deploy gate can reject stripped composites", async () => {
    const out = await packageSceneAssets({
      compositeText: compositeWith(["", "  ", "models/real.glb"]),
      fileKeys: ["scene.json", "models/real.glb"],
      fetchImpl: stubFetch({}),
    });
    expect(out.emptySrc).toBe(2);
    expect(out.missing).toEqual([]);
    expect(EMPTY_SRC_ERROR(2)).toContain("2 placed items");
    expect(EMPTY_SRC_ERROR(2)).toContain("empty GltfContainer src");
  });

  it("reports zero empty srcs for a healthy composite", async () => {
    const out = await packageSceneAssets({
      compositeText: compositeWith(["models/real.glb"]),
      fileKeys: ["models/real.glb"],
      fetchImpl: stubFetch({}),
    });
    expect(out.emptySrc).toBe(0);
  });
});

describe("buildDraftDeployFiles", () => {
  const fetchImpl = stubFetch({
    "/builder-items/": GLB_BYTES,
    "/template-bundles/games.js": RUNTIME_BYTES,
  });

  it("packages a template draft with composite, code edits, assets and the game runtime", async () => {
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
  });

  it("packages an empty draft with the idle runtime and no template tag", async () => {
    const pack = await buildDraftDeployFiles({ title: "Blank" }, { fetchImpl });
    expect(pack.runtimeInjected).toBe(true);
    expect(pack.metadata.tags).toEqual([]);
    const bin = pack.files.find((f) => f.file === "bin/index.js")!;
    expect(new TextDecoder().decode(bin.content)).toBe("game-runtime");
  });

  it("throws when the runtime bundle cannot be fetched", async () => {
    await expect(
      buildDraftDeployFiles({ title: "Blank" }, { fetchImpl: stubFetch({}) }),
    ).rejects.toThrow(/runtime/i);
  });

  it("sceneMainOf falls back to bin/index.js", () => {
    expect(sceneMainOf({})).toBe("bin/index.js");
    expect(sceneMainOf({ main: "bin/game.js" })).toBe("bin/game.js");
  });
});
