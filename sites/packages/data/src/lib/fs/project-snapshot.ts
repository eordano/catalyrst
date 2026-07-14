import { readDirectoryFiles } from "./disk";
import type { DeployFile } from "../catalyst/creator-hub/deploy-world";

export type ProjectSnapshot = {
  files: DeployFile[];
  title: string | null;
  sceneSize: string | null;
  parcels: string[];
  baseParcel: string | null;
  mainPresent: boolean | null;
};

export function parcelsToSize(parcels: readonly unknown[]): string | null {
  const xs: number[] = [];
  const ys: number[] = [];
  for (const p of parcels) {
    if (typeof p !== "string") continue;
    const [x, y] = p.split(",").map((v) => Number.parseInt(v.trim(), 10));
    if (Number.isFinite(x) && Number.isFinite(y)) {
      xs.push(x);
      ys.push(y);
    }
  }
  if (xs.length === 0) return null;
  return `${Math.max(...xs) - Math.min(...xs) + 1}x${Math.max(...ys) - Math.min(...ys) + 1}`;
}

export async function snapshotProjectDir(
  dir: FileSystemDirectoryHandle,
): Promise<ProjectSnapshot> {
  const map = await readDirectoryFiles(dir);
  const files: DeployFile[] = Object.entries(map)
    .map(([name, f]) => ({ name, size: f.size }))
    .sort((a, b) => a.name.localeCompare(b.name));

  let title: string | null = null;
  let sceneSize: string | null = null;
  let parcelList: string[] = [];
  let baseParcel: string | null = null;
  let mainPresent: boolean | null = null;
  const sceneFile = map["scene.json"];
  if (sceneFile) {
    try {
      const scene = JSON.parse(await sceneFile.text()) as {
        display?: { title?: unknown };
        scene?: { parcels?: unknown; base?: unknown };
        main?: unknown;
      };
      const t = scene?.display?.title;
      if (typeof t === "string" && t.trim()) title = t.trim();
      const parcels = scene?.scene?.parcels;
      if (Array.isArray(parcels)) {
        sceneSize = parcelsToSize(parcels);
        parcelList = parcels.filter(
          (p): p is string => typeof p === "string" && p.trim().length > 0,
        );
      }
      const base = scene?.scene?.base;
      if (typeof base === "string" && base.trim()) baseParcel = base.trim();
      const main =
        typeof scene?.main === "string" && scene.main.trim()
          ? scene.main.trim()
          : "bin/index.js";
      mainPresent = main in map;
    } catch {
    }
  }
  return { files, title, sceneSize, parcels: parcelList, baseParcel, mainPresent };
}
