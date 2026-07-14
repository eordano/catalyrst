import { z } from "zod";

import { CatalystError, getJSON, worldsBase } from "../client";
import type { GetOptions } from "../client";
import { worldsApiPath } from "../typed";
import { ETH_ADDRESS_RE } from "../format/address";

export const MAX_FILE_SIZE_MB = 50;

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

export function shortAddress(addr: string | null | undefined): string {
  const a = (addr ?? "").trim();
  return ETH_ADDRESS_RE.test(a) ? `${a.slice(0, 6)}\u{2026}${a.slice(-4)}` : a;
}

export const OwnedNameSchema = z.object({
  name: z.string(),
  contractAddress: z.string().nullish().transform((v) => v ?? null),
  tokenId: z
    .union([z.string(), z.number()])
    .nullish()
    .transform((v) => (v == null ? null : String(v))),
});
export type OwnedName = z.infer<typeof OwnedNameSchema>;

const NamesEnvelopeSchema = z.object({
  elements: z.array(z.unknown()),
  totalAmount: z.number(),
});

export type OwnedNamesPage = {
  elements: OwnedName[];
  total: number;
};

export async function fetchOwnedNames(
  address: string,
  opts: GetOptions = {},
): Promise<OwnedNamesPage> {
  const env = await getJSON<unknown>(
    `/lambdas/users/${encodeURIComponent(normalizeAddress(address))}/names`,
    { ...opts, query: { pageSize: 100 } },
  );
  const parsed = NamesEnvelopeSchema.safeParse(env);
  if (!parsed.success) {
    throw new CatalystError(
      "lambdas names response did not match the names-envelope shape",
      `/lambdas/users/${encodeURIComponent(normalizeAddress(address))}/names`,
    );
  }
  const elements: OwnedName[] = [];
  for (const raw of parsed.data.elements) {
    const r = OwnedNameSchema.safeParse(raw);
    if (r.success) elements.push(r.data);
  }
  return { elements, total: parsed.data.totalAmount };
}

const WorldsStatusSchema = z.object({
  ok: z.boolean().nullish(),
});

export async function fetchWorldsOnline(opts: GetOptions = {}): Promise<boolean> {
  const raw = await getJSON<unknown>(worldsApiPath("get", "/status"), {
    ...opts,
    base: worldsBase(),
  });
  const parsed = WorldsStatusSchema.safeParse(raw);
  return !parsed.success || parsed.data.ok !== false;
}

export type WorldInfo = {
  title: string;
  scenes: number;
  sizeMb: number;
  exists: boolean;
} | null;

export type DeployName = {
  name: string;
  provider: "dcl" | "ens";
  world: WorldInfo;
};

export type DeployFile = { name: string; size: number };

export type DeployWorldData = {
  address: string;
  names: DeployName[];
  liveEmpty: boolean;
  worldsOnline: boolean | null;
  project: { title: string; size: string; grad: string };
  files: DeployFile[];
  maxFileSizeMb: number;
  owner: {
    network: string;
    address: string;
    username: string;
    verified: boolean;
    role: string;
  };
  source: "live" | "empty";
};

export function totalBytes(files: ReadonlyArray<DeployFile>): number {
  return files.reduce((t, f) => t + (f.size || 0), 0);
}

export function exceedsQuota(files: ReadonlyArray<DeployFile>, maxMb = MAX_FILE_SIZE_MB): boolean {
  return totalBytes(files) > maxMb * 1e6 || files.some((f) => f.size > maxMb * 1e6);
}

export function formatSize(size: number): string {
  const KB = 1e3;
  const MB = 1e6;
  const GB = 1e9;
  if (size < KB) return `${size.toFixed(2)} B`;
  if (size < MB) return `${(size / KB).toFixed(2)} KB`;
  if (size < GB) return `${(size / MB).toFixed(2)} MB`;
  return `${(size / GB).toFixed(2)} GB`;
}

export function worldJumpUrl(name: string): string {
  return `https://catalyst.example.com/play/?realm=${encodeURIComponent(name)}`;
}

export function landJumpUrl(baseParcel: string): string {
  return `https://catalyst.example.com/play/?position=${encodeURIComponent(baseParcel)}`;
}
