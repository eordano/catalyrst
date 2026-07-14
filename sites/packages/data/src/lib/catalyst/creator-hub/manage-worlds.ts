import { z } from "zod";

export const NameSchema = z.object({
  name: z.string(),
  contractAddress: z.string().nullish().transform((v) => v ?? null),
  tokenId: z.string().nullish().transform((v) => v ?? null),
});
export type DclName = z.infer<typeof NameSchema>;

export const ManagedWorldSchema = z.object({
  name: z.string(),
  owner: z.string().nullish().transform((v) => v ?? null),
  title: z.string().nullish().transform((v) => v ?? null),
  description: z.string().nullish().transform((v) => v ?? null),
  contentRating: z.string().nullish().transform((v) => v ?? null),
  spawnCoordinates: z.string().nullish().transform((v) => v ?? null),
  lastDeployedAt: z.string().nullish().transform((v) => v ?? null),
  blockedSince: z.string().nullish().transform((v) => v ?? null),
  deployedScenes: z.number(),
  thumbnail: z.string().nullish().transform((v) => v ?? null),
  // How the viewer holds this world, and what the card's actions key off.
  // "owner" was the default, so a row that lost its role was handed the
  // strongest one there is -- every producer states it explicitly instead.
  role: z.enum(["owner", "collaborator", "operator"]),
});
export type ManagedWorld = z.infer<typeof ManagedWorldSchema>;

export const WalletStatsSchema = z.object({
  wallet: z.string(),
  dclNames: z.array(z.object({ name: z.string(), size: z.string() })),
  ensNames: z.array(z.object({ name: z.string(), size: z.string() })),
  usedSpace: z.string(),
  maxAllowedSpace: z.string(),
  blockedSince: z.string().nullish().transform((v) => v ?? null),
});
export type WalletStats = z.infer<typeof WalletStatsSchema>;

export const FILTERS = ["published", "unpublished"] as const;
export type WorldsFilter = (typeof FILTERS)[number];

export const SORTS = ["last_published", "domain"] as const;
export type WorldsSort = (typeof SORTS)[number];

export function readFilter(raw: string | null | undefined): WorldsFilter {
  return raw === "unpublished" ? "unpublished" : "published";
}

export function readSort(raw: string | null | undefined): WorldsSort {
  return raw === "domain" ? "domain" : "last_published";
}

export const SORT_TO_LABEL: Record<WorldsSort, string> = {
  last_published: "Last published",
  domain: "Domain name",
};
export const SORT_LABEL_TO_VALUE: Record<string, WorldsSort> = {
  "Last published": "last_published",
  "Domain name": "domain",
};

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

export function isPublished(w: ManagedWorld): boolean {
  return w.deployedScenes > 0;
}

export function applyFilter(
  worlds: ManagedWorld[],
  filter: WorldsFilter,
): ManagedWorld[] {
  return worlds.filter((w) =>
    filter === "published" ? isPublished(w) : !isPublished(w),
  );
}

export function applySearch(worlds: ManagedWorld[], search: string): ManagedWorld[] {
  const q = search.trim().toLowerCase();
  if (!q) return worlds;
  return worlds.filter(
    (w) =>
      w.name.toLowerCase().includes(q) ||
      (w.title ?? "").toLowerCase().includes(q),
  );
}

export function applySort(worlds: ManagedWorld[], sort: WorldsSort): ManagedWorld[] {
  const out = [...worlds];
  const time = (v: string | null) => (v ? Date.parse(v) || 0 : 0);
  switch (sort) {
    case "domain":
      return out.sort((a, b) => a.name.localeCompare(b.name));
    case "last_published":
    default:
      return out.sort((a, b) => time(b.lastDeployedAt) - time(a.lastDeployedAt));
  }
}

export type WorldCardVM = {
  id: string;
  displayName: string;
  type: "world" | "land";
  role: "owner" | "collaborator" | "operator";
  deployment: {
    title: string;
    scenesCount: number;
    grad: string;
    thumbnail: string | null;
  } | null;
};

function gradForName(name: string): string {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  const hue = h % 360;
  return `linear-gradient(135deg,hsl(${hue} 80% 55%) 0%,hsl(${(hue + 60) % 360} 70% 18%) 100%)`;
}

export function toWorldCard(w: ManagedWorld): WorldCardVM {
  return {
    id: w.name,
    displayName: w.name,
    type: "world",
    role: w.role,
    deployment: isPublished(w)
      ? {
          title: w.title ?? w.name,
          scenesCount: w.deployedScenes,
          grad: gradForName(w.name),
          thumbnail: w.thumbnail ?? null,
        }
      : null,
  };
}

export type StorageVM = {
  usedSpace: string;
  maxAllowedSpace: string;
  usedMb: number;
  maxMb: number;
  ownedLands: number;
  ownedNames: number;
  ownedMana: number;
};

const MB = 1024 * 1024;

export function parseManagedWorlds(raw: unknown): ManagedWorld[] {
  if (!Array.isArray(raw)) return [];
  const out: ManagedWorld[] = [];
  for (const item of raw) {
    const r = ManagedWorldSchema.safeParse(item);
    if (r.success) out.push(r.data);
  }
  return out;
}

export function parseNames(raw: unknown): DclName[] {
  if (!Array.isArray(raw)) return [];
  const out: DclName[] = [];
  for (const item of raw) {
    const r = NameSchema.safeParse(item);
    if (r.success) out.push(r.data);
  }
  return out;
}
