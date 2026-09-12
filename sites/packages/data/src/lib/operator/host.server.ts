import { statfs } from "node:fs/promises";
import { dirname } from "node:path";

import { envFilePath } from "./env-store.server";

export type DiskStatus = {
  path: string;
  totalBytes: number;
  freeBytes: number;
  usedPercent: number;
};

export async function diskStatus(): Promise<DiskStatus | null> {
  const envPath = envFilePath();
  const path = envPath ? dirname(envPath) : "/";
  try {
    const s = await statfs(path);
    const total = s.blocks * s.bsize;
    const free = s.bavail * s.bsize;
    if (!Number.isFinite(total) || total <= 0) return null;
    return {
      path,
      totalBytes: total,
      freeBytes: free,
      usedPercent: Math.round(((total - free) / total) * 100),
    };
  } catch {
    return null;
  }
}
