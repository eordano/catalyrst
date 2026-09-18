import { folderProject } from "./folder-project";
import type { DeWorkspaceCode } from "./types";

const FOLDER = "assets/custom-items";
export interface CustomItem { path: string; name: string }
export interface CustomItemStore {
  list(): Promise<string[]>;
  read(path: string): Promise<string>;
  write(path: string, content: string): Promise<void>;
}
export interface EntityCopy { version: 1; roots: string[]; nodes?: unknown[]; composite: string }

export async function customItemStore(code: DeWorkspaceCode): Promise<CustomItemStore> {
  if (code.project) return code.project;
  const directory = await code.getDir?.();
  if (directory) return folderProject(directory);
  if (!code.persist || !code.hydrate) throw new Error("Open a project to save reusable items.");
  return {
    async list() { return Object.keys(await code.hydrate!() ?? {}); },
    async read(path) {
      const content = (await code.hydrate!())?.[path];
      if (content === undefined) throw new Error("This custom item is unavailable. Refresh the library.");
      return content;
    },
    async write(path, content) { await code.persist!(path, content); },
  };
}

function fileName(path: string): string {
  if (!/^assets\/custom-items\/[a-z0-9][a-z0-9-]*\.composite$/.test(path)) throw new Error("Invalid custom item path");
  return path.slice(FOLDER.length + 1);
}
export function customItems(paths: string[]): CustomItem[] {
  return paths.filter(path => /^assets\/custom-items\/[a-z0-9][a-z0-9-]*\.composite$/.test(path)).sort().map(path => ({ path, name: fileName(path).replace(/\.composite$/, "").replaceAll("-", " ") }));
}
export function customItemPath(name: string, paths: string[]): string {
  const slug = name.normalize("NFKD").replace(/[\u0300-\u036f]/g, "").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 80) || "custom-item";
  let path = `${FOLDER}/${slug}.composite`;
  let suffix = 2;
  while (paths.includes(path)) path = `${FOLDER}/${slug}-${suffix++}.composite`;
  return path;
}
export function customItemPayload(composite: string): EntityCopy {
  const value = JSON.parse(composite) as { version?: number; components?: Array<{ name: string; data: Record<string, unknown> }> };
  if (value.version !== 1 || !Array.isArray(value.components)) throw new Error("This file is not a scene composite.");
  const unwrap = (raw: unknown) => raw && typeof raw === "object" && "json" in raw ? (raw as { json: unknown }).json : raw;
  const named = value.components.find(block => block.name === "core-schema::Name" || block.name === "core::Name" || block.name === "inspector::Name");
  const transforms = value.components.find(block => block.name === "core::Transform");
  const entities = Object.keys(named?.data ?? transforms?.data ?? {}).filter(id => Number.isSafeInteger(Number(id)) && Number(id) >= 512);
  const roots = entities.filter(id => !entities.includes(String((unwrap(transforms?.data[id]) as { parent?: number } | undefined)?.parent ?? 0)));
  if (!roots.length) throw new Error("The custom item contains no authored entities.");
  const nodes = value.components.find(block => block.name === "inspector::Nodes");
  return { version: 1, roots, composite, nodes: (unwrap(nodes?.data["0"]) as { value?: unknown[] } | undefined)?.value };
}
