export type DiscoveredScene = {
  entityId: string;
  kind: "land" | "world";
  title: string;
  baseParcel: string;
  pointers: string[];
  worldName: string | null;
  thumbnailUrl: string | null;
  editable: boolean;
  republishable: boolean;
  deployedAt: number | null;
  openHref: string;
};

export type RawContentEntry = { file?: string; key?: string; hash?: string };

export type RawDeployment = {
  entityId?: string;
  entityType?: string;
  pointers?: string[];
  content?: RawContentEntry[];
  metadata?: unknown;
  entityTimestamp?: number;
  localTimestamp?: number;
};

export const SCENE_CAP = 500;

const COMPOSITE_RE = /(^|\/)main\.composite$/;

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null;

function readString(v: unknown): string | null {
  return typeof v === "string" && v.trim() ? v : null;
}

function numOrNull(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

function contentFile(c: RawContentEntry): string {
  return c.file ?? c.key ?? "";
}

export function isLandPointer(p: string): boolean {
  const t = p.trim();
  return /^-?\d+,-?\d+$/.test(t) || t.includes(",");
}

function findContentHash(content: RawContentEntry[], path: string): string | null {
  const target = path.trim();
  if (!target) return null;
  for (const c of content) {
    if (contentFile(c) === target && c.hash) return c.hash;
  }
  const lower = target.toLowerCase();
  for (const c of content) {
    if (contentFile(c).toLowerCase() === lower && c.hash) return c.hash;
  }
  return null;
}

function stripBase(base: string): string {
  return base.replace(/\/+$/, "");
}

function buildOpenHref(
  kind: "land" | "world",
  pointer: string,
): string {
  const q = new URLSearchParams();
  q.set("pointer", pointer);
  if (kind === "world") q.set("world", "1");
  q.set("from", "discovery");
  return `/creator-hub/scene-editor?${q.toString()}`;
}

export function normalizeDeployment(
  d: RawDeployment,
  base: string,
): DiscoveredScene | null {
  if (d.entityType !== "scene") return null;
  const entityId = readString(d.entityId);
  if (!entityId) return null;

  const cleanBase = stripBase(base);
  const pointers = Array.isArray(d.pointers)
    ? d.pointers.filter((p): p is string => typeof p === "string")
    : [];
  const content = Array.isArray(d.content) ? d.content : [];

  const md = isObj(d.metadata) ? d.metadata : {};
  const display = isObj(md.display) ? md.display : {};
  const sceneMeta = isObj(md.scene) ? md.scene : {};
  const worldCfg = isObj(md.worldConfiguration) ? md.worldConfiguration : {};

  const kind: "land" | "world" = pointers.some(isLandPointer) ? "land" : "world";

  const worldName =
    kind === "world"
      ? pointers.find((p) => !isLandPointer(p)) ?? readString(worldCfg.name)
      : null;

  const baseParcel =
    kind === "land"
      ? readString(sceneMeta.base) ?? pointers[0] ?? ""
      : worldName ?? "";

  const title =
    readString(display.title) ?? readString(sceneMeta.title) ?? "Untitled scene";

  const thumbPath =
    readString(display.navmapThumbnail) ?? readString(md.navmapThumbnail);
  const thumbHash = thumbPath ? findContentHash(content, thumbPath) : null;
  const thumbnailUrl = thumbHash
    ? `${cleanBase}/content/contents/${thumbHash}`
    : null;

  const editable = content.some((c) => COMPOSITE_RE.test(contentFile(c)));

  const republishable = kind === "world" || pointers.every(isLandPointer);

  const deployedAt = numOrNull(d.entityTimestamp) ?? numOrNull(d.localTimestamp);

  const openHref = buildOpenHref(
    kind,
    kind === "world" ? worldName ?? "" : baseParcel,
  );

  return {
    entityId,
    kind,
    title,
    baseParcel,
    pointers,
    worldName,
    thumbnailUrl,
    editable,
    republishable,
    deployedAt,
    openHref,
  };
}

function sortScenes(a: DiscoveredScene, b: DiscoveredScene): number {
  if (a.editable !== b.editable) return a.editable ? -1 : 1;
  if (a.deployedAt == null && b.deployedAt == null) return 0;
  if (a.deployedAt == null) return 1;
  if (b.deployedAt == null) return -1;
  return b.deployedAt - a.deployedAt;
}

export function normalizeDeployments(
  deployments: RawDeployment[],
  base: string,
): DiscoveredScene[] {
  const seen = new Set<string>();
  const out: DiscoveredScene[] = [];
  for (const raw of deployments) {
    const scene = normalizeDeployment(raw, base);
    if (!scene) continue;
    if (seen.has(scene.entityId)) continue;
    seen.add(scene.entityId);
    out.push(scene);
  }
  out.sort(sortScenes);
  return out;
}
