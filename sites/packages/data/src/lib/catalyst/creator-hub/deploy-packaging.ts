import { buildScaffoldFiles } from "../../fs/scaffold-project";
import { SEEDED_ASSET_SRC_RE } from "../../fs/save-scene";
import type { DeployContentFile } from "./deploy-scene";

export const SEEDED_ASSET_DIR = "assets/imported/template-assets";
export const GAME_RUNTIME_URL = "/template-bundles/games.js";
export const DEFAULT_SCENE_MAIN = "bin/index.js";

export const RUNTIME_NOTE =
  "This publish includes your src/ code files, but the deployed world runs the " +
  "prebuilt template runtime (bin/index.js) \u{2014} the same one the editor's Play " +
  "button uses. Code edits ship as sources and do not change what runs; to run " +
  "custom code, build the project locally (npm install && npm run build) so " +
  "bin/index.js exists before publishing.";

const ABS_BUILDER_SRC_RE = /^(?:https?:\/\/[^/]+)?\/builder-items\/([A-Za-z0-9]+)$/;

const IMPORTED_ITEM_SRC_RE =
  /^assets\/imported\/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\/(.+)$/i;

const ASSET_PACKS_URL = "/builder-api/v1/assetPacks";

export function extractGltfSrcHolders(
  composite: unknown,
): { obj: Record<string, unknown>; src: string }[] {
  const out: { obj: Record<string, unknown>; src: string }[] = [];
  const components =
    composite && typeof composite === "object"
      ? (composite as { components?: unknown }).components
      : undefined;
  if (!Array.isArray(components)) return out;
  for (const block of components) {
    if (!block || typeof block !== "object") continue;
    const name = (block as { name?: unknown }).name;
    if (typeof name !== "string" || !/GltfContainer$/.test(name)) continue;
    const data = (block as { data?: unknown }).data;
    if (!data || typeof data !== "object") continue;
    for (const entry of Object.values(data as Record<string, unknown>)) {
      const val =
        entry && typeof entry === "object" && "json" in (entry as object)
          ? (entry as { json?: unknown }).json
          : entry;
      if (!val || typeof val !== "object") continue;
      const src = (val as { src?: unknown }).src;
      if (typeof src === "string" && src.trim()) {
        out.push({ obj: val as Record<string, unknown>, src: src.trim() });
      }
    }
  }
  return out;
}

export function relativeAssetPresent(fileKeys: readonly string[], src: string): boolean {
  const norm = src.replace(/^\.?\//, "");
  if (!norm) return false;
  const suffix = `/${norm}`;
  return fileKeys.some((k) => k === norm || k.endsWith(suffix));
}

export function countEmptyGltfSrcs(composite: unknown): number {
  const components =
    composite && typeof composite === "object"
      ? (composite as { components?: unknown }).components
      : undefined;
  if (!Array.isArray(components)) return 0;
  let empty = 0;
  for (const block of components) {
    if (!block || typeof block !== "object") continue;
    const name = (block as { name?: unknown }).name;
    if (typeof name !== "string" || !/GltfContainer$/.test(name)) continue;
    const data = (block as { data?: unknown }).data;
    if (!data || typeof data !== "object") continue;
    for (const entry of Object.values(data as Record<string, unknown>)) {
      const val =
        entry && typeof entry === "object" && "json" in (entry as object)
          ? (entry as { json?: unknown }).json
          : entry;
      if (!val || typeof val !== "object") continue;
      const src = (val as { src?: unknown }).src;
      if (typeof src !== "string" || !src.trim()) empty += 1;
    }
  }
  return empty;
}

export const EMPTY_SRC_ERROR = (count: number): string =>
  `Cannot publish: ${count} placed item${count === 1 ? "" : "s"} reference no ` +
  `model file (empty GltfContainer src) \u{2014} this usually means the scene was ` +
  `saved while the 3D engine was unavailable. Re-open the scene in the editor, ` +
  `wait for the viewport to load, and Save again before publishing.`;

export type PackagedAssets = {
  compositeText: string;
  changed: boolean;
  extra: DeployContentFile[];
  missing: string[];
  emptySrc: number;
};

export async function packageSceneAssets(input: {
  compositeText: string;
  fileKeys: readonly string[];
  fetchImpl?: typeof fetch;
}): Promise<PackagedAssets> {
  const doFetch = input.fetchImpl ?? fetch;
  const none: PackagedAssets = {
    compositeText: input.compositeText,
    changed: false,
    extra: [],
    missing: [],
    emptySrc: 0,
  };
  let composite: unknown;
  try {
    composite = JSON.parse(input.compositeText);
  } catch {
    return none;
  }
  const emptySrc = countEmptyGltfSrcs(composite);
  const holders = extractGltfSrcHolders(composite);
  if (holders.length === 0) return { ...none, emptySrc };

  const extraByPath = new Map<string, DeployContentFile>();
  const missing: string[] = [];
  const failedCids = new Set<string>();
  let changed = false;

  let importedItems: Map<string, Record<string, string>> | null = null;
  const itemContentsOf = async (
    id: string,
  ): Promise<Record<string, string> | undefined> => {
    if (!importedItems) {
      importedItems = new Map();
      try {
        const res = await doFetch(ASSET_PACKS_URL, { credentials: "omit" });
        if (res.ok) {
          const body = (await res.json()) as {
            data?: { assets?: { id?: string; contents?: Record<string, string> }[] }[];
          };
          for (const pack of body.data ?? []) {
            for (const asset of pack.assets ?? []) {
              if (asset.id && asset.contents) {
                importedItems.set(asset.id.trim(), asset.contents);
              }
            }
          }
        }
      } catch {
      }
    }
    return importedItems.get(id);
  };

  const ensureBuilderFile = async (cid: string, path: string): Promise<boolean> => {
    if (extraByPath.has(path) || relativeAssetPresent(input.fileKeys, path)) return true;
    if (failedCids.has(cid)) return false;
    try {
      const res = await doFetch(`/builder-items/${cid}`, { credentials: "omit" });
      if (!res.ok) {
        failedCids.add(cid);
        return false;
      }
      extraByPath.set(path, {
        file: path,
        content: new Uint8Array(await res.arrayBuffer()),
      });
      return true;
    } catch {
      failedCids.add(cid);
      return false;
    }
  };

  for (const h of holders) {
    const abs = ABS_BUILDER_SRC_RE.exec(h.src);
    if (abs) {
      const cid = abs[1];
      const path = `${SEEDED_ASSET_DIR}/${cid}.glb`;
      if (await ensureBuilderFile(cid, path)) {
        h.obj.src = path;
        changed = true;
      } else {
        missing.push(h.src);
      }
      continue;
    }
    const seeded = SEEDED_ASSET_SRC_RE.exec(h.src);
    if (seeded) {
      if (!(await ensureBuilderFile(seeded[1], h.src))) missing.push(h.src);
      continue;
    }
    const imported = IMPORTED_ITEM_SRC_RE.exec(h.src);
    if (imported) {
      if (relativeAssetPresent(input.fileKeys, h.src)) continue;
      const contents = await itemContentsOf(imported[1]);
      const refKey = contents
        ? Object.keys(contents).find(
            (k) => k.toLowerCase() === imported[2].toLowerCase(),
          )
        : undefined;
      if (!contents || !refKey || !(await ensureBuilderFile(contents[refKey], h.src))) {
        missing.push(h.src);
        continue;
      }
      const dir = h.src.slice(0, h.src.length - imported[2].length);
      for (const [file, hash] of Object.entries(contents)) {
        if (file !== refKey) await ensureBuilderFile(hash, dir + file);
      }
      continue;
    }
    if (/^https?:\/\//i.test(h.src)) continue;
    if (h.src.startsWith("/") || !relativeAssetPresent(input.fileKeys, h.src)) {
      missing.push(h.src);
    }
  }

  let compositeText = input.compositeText;
  if (changed) {
    try {
      compositeText = JSON.stringify(composite);
    } catch {
      compositeText = input.compositeText;
      changed = false;
    }
  }
  return {
    compositeText,
    changed,
    extra: Array.from(extraByPath.values()),
    missing: Array.from(new Set(missing)),
    emptySrc,
  };
}

export async function fetchGameRuntime(
  fetchImpl?: typeof fetch,
): Promise<Uint8Array | null> {
  try {
    const doFetch = fetchImpl ?? fetch;
    const res = await doFetch(GAME_RUNTIME_URL, { credentials: "omit" });
    if (!res.ok) return null;
    return new Uint8Array(await res.arrayBuffer());
  } catch {
    return null;
  }
}

export function sceneMainOf(metadata: unknown): string {
  const main =
    metadata && typeof metadata === "object"
      ? (metadata as { main?: unknown }).main
      : undefined;
  return typeof main === "string" && main.trim() ? main.trim() : DEFAULT_SCENE_MAIN;
}

export type DraftProjectMeta = {
  title: string;
  base?: string;
  template?: string;
  composite?: string;
  codeFiles?: Record<string, string>;
};

export type DraftPackage = {
  files: DeployContentFile[];
  metadata: Record<string, unknown>;
  runtimeInjected: boolean;
  missing: string[];
  emptySrc: number;
};

const utf8 = (text: string) => new TextEncoder().encode(text);

export async function buildDraftDeployFiles(
  meta: DraftProjectMeta,
  opts: { fetchImpl?: typeof fetch } = {},
): Promise<DraftPackage> {
  const scaffold = buildScaffoldFiles({
    name: meta.title,
    template: meta.template,
    parcels: [meta.base?.trim() || "0,0"],
  });
  const byPath = new Map<string, Uint8Array>();
  for (const f of scaffold) byPath.set(f.path, utf8(f.text));
  if (meta.composite?.trim()) byPath.set("main.composite", utf8(meta.composite));
  for (const [path, text] of Object.entries(meta.codeFiles ?? {})) {
    if (typeof text === "string") byPath.set(path, utf8(text));
  }

  const sceneJsonBytes = byPath.get("scene.json");
  const metadata = JSON.parse(
    new TextDecoder().decode(sceneJsonBytes ?? utf8("{}")),
  ) as Record<string, unknown>;

  const compositeBytes = byPath.get("main.composite");
  let missing: string[] = [];
  let emptySrc = 0;
  if (compositeBytes) {
    const packaged = await packageSceneAssets({
      compositeText: new TextDecoder().decode(compositeBytes),
      fileKeys: Array.from(byPath.keys()),
      fetchImpl: opts.fetchImpl,
    });
    if (packaged.changed) byPath.set("main.composite", utf8(packaged.compositeText));
    for (const f of packaged.extra) byPath.set(f.file, f.content);
    missing = packaged.missing;
    emptySrc = packaged.emptySrc;
  }

  const main = sceneMainOf(metadata);
  let runtimeInjected = false;
  if (!byPath.has(main)) {
    const runtime = await fetchGameRuntime(opts.fetchImpl);
    if (!runtime) {
      throw new Error(
        "The prebuilt scene runtime is unavailable \u{2014} cannot package this draft for publishing right now.",
      );
    }
    byPath.set(main, runtime);
    runtimeInjected = true;
  }

  return {
    files: Array.from(byPath, ([file, content]) => ({ file, content })),
    metadata,
    runtimeInjected,
    missing,
    emptySrc,
  };
}
