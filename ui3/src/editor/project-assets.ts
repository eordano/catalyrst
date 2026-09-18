import type { DeCatalogItem, ProjectAssets } from "./types";
import { PROJECT_CACHE } from "./project-cache-name";

export function safeAssetPath(path: string): string {
  if (!path || path.startsWith("/") || path.includes("\\") || path.includes("\0") || path.split("/").some(part => !part || part === "." || part === ".." || part.startsWith("."))) throw new Error(`Invalid asset path: ${path}`);
  return path;
}
function equalBytes(left: ArrayBuffer, right: ArrayBuffer): boolean {
  const a = new Uint8Array(left), b = new Uint8Array(right);
  return a.length === b.length && a.every((value, index) => value === b[index]);
}
export async function persistCatalogAsset(store: ProjectAssets, asset: DeCatalogItem, request: typeof fetch = fetch): Promise<string> {
  const id = String(asset.id || "asset").replace(/[^a-zA-Z0-9_-]/g, "-").slice(0, 120);
  const base = `assets/imported/${id}`;
  const model = safeAssetPath(asset.glbFile || "model.glb");
  const sources = Object.entries(asset.contents ?? {});
  if (!sources.length) {
    const url = asset.glbUrl || asset.src;
    if (!url) throw new Error("This asset has no model file to save.");
    sources.push([model, url]);
  } else if (!sources.some(([path]) => path === model)) throw new Error("The asset's file list does not contain its model.");
  for (const [path] of sources) safeAssetPath(path);
  const existing = new Set((await store.list()).map(file => file.path));
  for (const [relative, source] of sources) {
    const path = `${base}/${relative}`;
    const url = /^[a-zA-Z0-9]+$/.test(source) ? `/builder-items/${source}` : source;
    const local = typeof caches === "undefined" ? undefined
      : await caches.open(PROJECT_CACHE).then(cache => cache.match(url)).catch(() => undefined);
    const response = local ?? await request(url, { credentials: "omit", signal: AbortSignal.timeout(30000) });
    if (!response.ok) throw new Error(`Could not download ${relative} (${response.status}).`);
    const content = await response.arrayBuffer();
    if (existing.has(path)) {
      const saved = await store.read(path);
      if (!equalBytes(content, saved.content)) throw new Error(`${path} already contains a different asset. Rename it before importing this item.`);
    } else await store.write(path, content);
  }
  return `${base}/${model}`;
}

export interface AssetCandidate { path: string; size: number }
export async function findUnreferencedAssets(project: { list(): Promise<string[]>; read(path: string): Promise<string>; readOnly?(path: string): Promise<string>; assets: ProjectAssets }, liveComposite: string): Promise<AssetCandidate[]> {
  const assets = await project.assets.list();
  const documents = await project.list();
  const sourcePaths = documents.filter(path => /\.(?:[cm]?[jt]sx?|json|composite|glsl)$/i.test(path));
  const sources = await Promise.all(sourcePaths.map(path => project.readOnly ? project.readOnly(path) : project.read(path)));
  sources.push(liveComposite);
  const used = new Set<string>();
  const mentions = (text: string, path: string) => text.includes(path) || text.includes(path.split("/").at(-1)!);
  let progress = true;
  while (progress) {
    progress = false;
    for (const asset of assets) {
      if (used.has(asset.path) || !sources.some(text => mentions(text, asset.path))) continue;
      used.add(asset.path);
      progress = true;
      if (/\.gltf$/i.test(asset.path)) sources.push(new TextDecoder().decode((await project.assets.read(asset.path)).content));
      if (/\.glb$/i.test(asset.path)) {
        const buffer = (await project.assets.read(asset.path)).content;
        if (buffer.byteLength < 20) throw new Error(`Cannot inspect model dependencies in ${asset.path}.`);
        const view = new DataView(buffer);
        if (view.getUint32(0, true) !== 0x46546c67 || view.getUint32(16, true) !== 0x4e4f534a || view.getUint32(12, true) > buffer.byteLength - 20) throw new Error(`Cannot inspect model dependencies in ${asset.path}.`);
        sources.push(new TextDecoder().decode(buffer.slice(20, 20 + view.getUint32(12, true))));
      }
    }
  }
  return assets.filter(asset => !used.has(asset.path) && !/\.composite$/i.test(asset.path));
}
