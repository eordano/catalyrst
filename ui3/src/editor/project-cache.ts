import type { RefObject } from "react";
import type { EditorBus } from "./editor-bus";
import { persistCatalogAsset } from "./project-assets";
import type { DeCatalogItem, ProjectAssets } from "./types";

const BUILDER_ITEMS_PREFIX = "/builder-items/";

import { PROJECT_CACHE } from "./project-cache-name";
export { PROJECT_CACHE } from "./project-cache-name";

let _projectContentBase: string | null | undefined;
export async function projectContentBase(): Promise<string | null> {
  if (_projectContentBase !== undefined) return _projectContentBase;
  _projectContentBase = null;
  try {
    const res = await fetch("/_project/about", { cache: "no-store" });
    if (res.ok) {
      const about = await res.json();
      const pub = about && about.content && about.content.publicUrl;
      if (typeof pub === "string" && pub) _projectContentBase = pub.replace(/\/$/, "");
    }
  } catch {
  }
  return _projectContentBase;
}

export async function setProjectPlayState(playing: boolean, debug = false): Promise<void> {
  if (typeof caches === "undefined") return;
  try {
    const base = await projectContentBase();
    if (!base) return;
    const cache = await caches.open(PROJECT_CACHE);
    await cache.put(
      `${base}/contents/one-play-state`,
      new Response(JSON.stringify({ playing, ...(debug ? { debug } : {}) }), {
        headers: {
          "content-type": "application/json",
          "access-control-allow-origin": "*",
        },
      }),
    );
  } catch {
  }
}

interface InitAssetResult {
  baseDir?: string;
  hashes?: Record<string, string>;
}

async function mirrorContentsToProjectCache(
  contents: Record<string, string>,
  persisted: InitAssetResult,
): Promise<void> {
  if (typeof caches === "undefined") return;
  try {
    const base = await projectContentBase();
    const cache = await caches.open(PROJECT_CACHE).catch(() => null);
    if (!base || !cache) return;
    const baseDir = String(persisted.baseDir || "").replace(/\/+$/, "");
    const hashes: Record<string, string> =
      persisted.hashes && typeof persisted.hashes === "object" ? persisted.hashes : {};
    await Promise.all(
      Object.entries(contents).map(async ([path, cid]) => {
        if (typeof cid !== "string" || !cid) return;
        try {
          const r = await fetch(BUILDER_ITEMS_PREFIX + cid, { credentials: "omit" });
          if (!r.ok) return;
          const buf = await r.arrayBuffer();
          const put = (h: string) =>
            cache.put(
              base + "/contents/" + h,
              new Response(buf.slice(0), {
                headers: {
                  "content-type": "application/octet-stream",
                  "access-control-allow-origin": "*",
                },
              }),
            );
          await put(cid);
          const mh = hashes[(baseDir + "/" + path).toLowerCase()];
          if (mh && mh !== cid) await put(mh);
        } catch {
        }
      }),
    );
  } catch {
  }
}

const TEMPLATE_ASSET_DIR = "assets/imported/template-assets";
const BUILDER_SRC_RE = /(?:^|\/)builder-items\/([A-Za-z0-9]+)$/;

const placedContents: Record<string, string> = {};

export function placedProjectContents(): Record<string, string> {
  return { ...placedContents };
}

function compositeGltfHolders(
  composite: unknown,
): { obj: Record<string, unknown>; src: string }[] {
  const holders: { obj: Record<string, unknown>; src: string }[] = [];
  const components =
    composite && typeof composite === "object"
      ? (composite as { components?: unknown }).components
      : undefined;
  if (!Array.isArray(components)) return holders;
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
      if (typeof src === "string" && src) {
        holders.push({ obj: val as Record<string, unknown>, src });
      }
    }
  }
  return holders;
}

export function resolveCompositeAssets(compositeText: string): string {
  let composite: unknown;
  try {
    composite = JSON.parse(compositeText);
  } catch {
    return compositeText;
  }
  const holders = compositeGltfHolders(composite)
    .map((h) => ({ obj: h.obj, cid: BUILDER_SRC_RE.exec(h.src)?.[1] ?? null }))
    .filter((h): h is { obj: Record<string, unknown>; cid: string } => h.cid !== null);
  if (holders.length === 0) return compositeText;
  for (const h of holders) h.obj.src = `${TEMPLATE_ASSET_DIR}/${h.cid}.glb`;
  try {
    return JSON.stringify(composite);
  } catch {
    return compositeText;
  }
}

export interface PlaceAssetOutcome {
  name: string;
  mirrored: boolean;
  warning: string | null;
}

export async function registerProjectContents(busRef: RefObject<EditorBus | null>, assets?: ProjectAssets): Promise<void> {
  if (!assets?.preparePreview) return;
  const bus = busRef.current;
  if (!bus) throw new Error("The scene is not connected yet.");
  const contents = await assets.preparePreview();
  if (busRef.current !== bus) throw new Error("The editor reconnected while preparing assets. Try again.");
  const count = Object.keys(contents).length;
  if (count && await bus.rpc("registerContent", [contents], 30000) !== count) {
    throw new Error("The renderer did not register every project asset. Reconnect and try again.");
  }
}

export async function placeAssetOnBus(
  busRef: RefObject<EditorBus | null>,
  asset: DeCatalogItem,
  drop?: { x: number; y: number } | null,
  projectAssets?: ProjectAssets,
): Promise<PlaceAssetOutcome> {
  const bus = busRef.current;
  if (!bus) throw new Error("the scene is not connected yet");
  if (projectAssets) {
    const src = await persistCatalogAsset(projectAssets, asset);
    await registerProjectContents(busRef, projectAssets);
    if (busRef.current !== bus) throw new Error("The editor reconnected during the import. Place the item again.");
    await createPlacedEntity(bus, asset.name || "Item", { GltfContainer: { src } }, drop ?? null);
    return { name: asset.name || "Item", mirrored: true, warning: null };
  }
  const glb = asset?.glbUrl || asset?.src;
  let absUrl: string | null = null;
  if (typeof glb === "string" && glb) {
    absUrl = glb;
    try {
      if (typeof window !== "undefined") absUrl = new URL(glb, window.location.origin).href;
    } catch {
    }
  }

  const contents =
    asset?.contents && typeof asset.contents === "object" ? asset.contents : null;
  let persisted: InitAssetResult | null = null;
  let warning: string | null = null;
  if (asset?.id && contents && busRef.current && typeof busRef.current.rpc === "function") {
    try {
      persisted = (await busRef.current.rpc("initAsset", [asset.id, contents], 15000)) as InitAssetResult;
    } catch (e) {
      persisted = null;
      const reason = e instanceof Error ? e.message : String(e);
      warning = absUrl
        ? `its files could not be copied into the project (${reason}); the model links to the catalog instead`
        : `its files could not be copied into the project (${reason})`;
    }
  }

  if (persisted && contents) {
    const baseDir = String(persisted.baseDir || "").replace(/\/+$/, "");
    if (baseDir) {
      for (const [path, cid] of Object.entries(contents as Record<string, string>)) {
        if (typeof cid === "string" && cid) placedContents[`${baseDir}/${path}`] = cid;
      }
    }
    await mirrorContentsToProjectCache(contents as Record<string, string>, persisted);
  }

  let src = absUrl;
  if (
    persisted &&
    typeof persisted.baseDir === "string" &&
    persisted.baseDir &&
    asset?.glbFile
  ) {
    src = persisted.baseDir.replace(/\/+$/, "") + "/" + asset.glbFile;
  }

  const components = src ? { GltfContainer: { src } } : null;
  if (!components && !warning) warning = "it has no model file, so an empty item was placed";
  const name = asset?.name || "Item";
  if (busRef.current !== bus) throw new Error("The editor reconnected during the import. Place the item again.");
  await createPlacedEntity(bus, name, components, drop ?? null);
  return { name, mirrored: persisted !== null, warning };
}

async function createPlacedEntity(bus: EditorBus, name: string, components: Record<string, unknown> | null, drop: { x: number; y: number } | null): Promise<void> {
  const entity = await bus.rpc("addEntity", [name, 0, components, drop]);
  if (typeof entity !== "string" || !Number.isSafeInteger(Number(entity)) || Number(entity) < 512) throw new Error("The scene did not acknowledge a placed entity. Reconnect and try again.");
}
