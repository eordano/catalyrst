import { privateKeyToAccount } from "viem/accounts";

import type { AuthLink } from "../auth/identity";
import { loadStoredIdentity } from "../auth/signedFetchLocal";
import { getJSON, catalystBase, type RequestOpts } from "./client";
import { isRecord, keepRow, keepRows } from "./rows";
import {
  CategorySchema,
  EmoteSchema,
  EquippedSchema,
  OwnedElementSchema,
  OwnedEmoteElementSchema,
  SlotBindingSchema,
  WearableSchema,
} from "./schemas/backpack";
import type {
  EmoteWire,
  EquippedWire,
  SlotBindingWire,
  WearableWire,
  WearableCategory,
} from "./schemas/backpack";
import { bucketEmoteCategory, SLOT_ORDER } from "./taxonomy";

export {
  CategorySchema,
  EmoteSchema,
  EquippedSchema,
  OwnedElementSchema,
  OwnedEmoteElementSchema,
  SlotBindingSchema,
  WearableSchema,
};
export type { WearableCategory };
export { EMOTE_CATEGORIES, RARITIES, SLOT_ORDER, WEARABLE_CATEGORIES } from "./taxonomy";

// The wire -> exported-type normalization, which the schema module cannot
// carry: a perf build replaces it with an accepting stub, and a stub reproduces
// no transform. Here it runs whether or not validation did.
//
// Every field a schema marks nullish is restated, so each type below is exactly
// what a caller gets and a field added to a schema shows up as its honest
// `| undefined` until it is normalized here too.

export function normalizeWearable(w: WearableWire) {
  return {
    ...w,
    thumbnail: w.thumbnail ?? null,
    description: w.description ?? null,
    creator: w.creator ?? null,
    network: w.network ?? null,
  };
}

export type Wearable = ReturnType<typeof normalizeWearable>;

export function normalizeSlotBinding(b: SlotBindingWire) {
  return { ...b, name: b.name ?? null };
}

export type SlotBinding = ReturnType<typeof normalizeSlotBinding>;

/**
 * The nulls here are the ones `UNKNOWN_EQUIPPED` is made of: `fetchEquipped`
 * deliberately builds its candidate with `|| undefined`, so without this an
 * unreadable profile and a readable one that says nothing stop matching.
 */
export function normalizeEquipped(e: EquippedWire) {
  return {
    ...e,
    bodyShape: e.bodyShape ?? null,
    skinColor: e.skinColor ?? null,
    hairColor: e.hairColor ?? null,
    eyeColor: e.eyeColor ?? null,
    name: e.name ?? null,
    wearables: e.wearables ?? null,
    emotes: e.emotes ?? null,
    emoteSlots: e.emoteSlots?.map(normalizeSlotBinding) ?? null,
  };
}

export type Equipped = ReturnType<typeof normalizeEquipped>;

export function normalizeEmote(e: EmoteWire) {
  return {
    ...e,
    description: e.description ?? null,
    thumbnail: e.thumbnail ?? null,
    rarity: e.rarity ?? null,
    category: bucketEmoteCategory(e.category ?? null),
    loop: e.loop ?? null,
  };
}

export type Emote = ReturnType<typeof normalizeEmote>;

export function normalizeAddress(addr?: string | null): string {
  return (addr ?? "").trim().toLowerCase();
}

export function isEthAddress(addr?: string | null): boolean {
  return /^0x[0-9a-fA-F]{40}$/.test((addr ?? "").trim());
}

export function baseItemUrn(urn: string): string {
  const p = String(urn).split(":");
  return p.length === 7 && /^collections-v[12]$/.test(p[3] ?? "")
    ? p.slice(0, 6).join(":")
    : urn;
}

export function isSlotNumber(n: unknown): boolean {
  return typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= 9;
}

// What the backpack readers can USE, as opposed to what their schemas declare.
// A perf build strips the schemas and keeps the row; these keep the row from
// reaching a consumer that cannot survive it. See rows.ts.

/**
 * `byCategory` indexes on `category`, `rarityLabel` calls `.charAt` on `rarity`,
 * `findWearable` and the owned set match on `urn`, and the equip path iterates
 * `bodyShapes`. The remaining four fields are nullish and already normalized,
 * which is why they are absent here -- schemas/backpack.ts says the same of the
 * checking build, in the same words: an item that cannot supply one of these is
 * an item nothing can render.
 */
export function isRenderableWearable(row: unknown): boolean {
  return (
    isRecord(row) &&
    typeof row.urn === "string" &&
    row.urn !== "" &&
    typeof row.name === "string" &&
    typeof row.rarity === "string" &&
    typeof row.category === "string" &&
    Array.isArray(row.bodyShapes)
  );
}

/** An emote is addressed by `urn` and labelled by `name`; the rest normalizes. */
export function isRenderableEmote(row: unknown): boolean {
  return (
    isRecord(row) &&
    typeof row.urn === "string" &&
    row.urn !== "" &&
    typeof row.name === "string"
  );
}

/**
 * `sortLoadout` ranks by `slot` and the wheel positions by it, so a non-number
 * makes the comparator NaN and puts the binding wherever the service happened to
 * send it -- the wrong emote under a key the user pressed on purpose. `urn` is
 * what actually plays.
 */
export function isUsableSlotBinding(row: unknown): boolean {
  return (
    isRecord(row) &&
    isSlotNumber(row.slot) &&
    typeof row.urn === "string" &&
    row.urn !== ""
  );
}

/** The owned lists keep nothing but the urn, so that is the whole requirement. */
function hasUrn(row: unknown): boolean {
  return isRecord(row) && typeof row.urn === "string";
}

const UNKNOWN_EQUIPPED: Equipped = {
  bodyShape: null,
  skinColor: null,
  hairColor: null,
  eyeColor: null,
  name: null,
  wearables: null,
  emotes: null,
  emoteSlots: null,
};

interface RawContentFile {
  file?: string;
  hash?: string;
}
interface RawRepresentation {
  bodyShapes?: unknown;
  mainFile?: string;
}
interface RawEmoteData {
  category?: string;
  loop?: boolean;
  representations?: RawRepresentation[];
}
interface RawWearableData {
  category?: string;
  representations?: unknown;
  requiredPermissions?: unknown;
}
interface RawMetadata {
  id?: string;
  name?: string;
  thumbnail?: string;
  image?: string;
  rarity?: string;
  description?: string;
  data?: RawWearableData;
  emoteDataADR74?: RawEmoteData;
  i18n?: unknown;
}
interface RawEntity {
  id?: string;
  metadata?: RawMetadata;
  content?: unknown;
  pointers?: string[];
}
interface RawElement {
  urn?: string;
  id?: string;
  category?: string;
  name?: string;
  type?: string;
  entity?: RawEntity;
}
interface RawColorHolder {
  color?: unknown;
}
interface RawAvatarInner {
  bodyShape?: string;
  skin?: RawColorHolder;
  hair?: RawColorHolder;
  eyes?: RawColorHolder;
  wearables?: unknown;
  emotes?: unknown;
}
interface RawProfileAvatar {
  name?: string;
  avatar?: RawAvatarInner;
}
interface RawProfileEnv {
  avatars?: RawProfileAvatar[];
}
interface RawOutfitEntry {
  slot?: number;
  outfit?: {
    bodyShape?: string;
    wearables?: unknown;
    skin?: RawColorHolder;
    hair?: RawColorHolder;
    eyes?: RawColorHolder;
  };
}
interface RawOutfitsEnv {
  metadata?: { outfits?: RawOutfitEntry[] };
  outfits?: RawOutfitEntry[];
}
interface RawDeployment {
  metadata?: { avatars?: RawProfileAvatar[] };
  pointers?: string[];
}
interface RawEmoteInput {
  urn?: string;
  id?: string;
  name?: string;
  description?: string;
  thumbnail?: string;
  rarity?: string;
  emoteDataADR74?: RawEmoteData;
}

function asStringArray(v: unknown): string[] {
  return Array.isArray(v) ? (v as string[]) : [];
}

export function parseCatalog(raw: unknown): Wearable[] {
  return keepRows(raw, WearableSchema, isRenderableWearable, normalizeWearable);
}

export function parseOwned(raw: unknown): string[] {
  return keepRows(raw, OwnedElementSchema, hasUrn, (r) => r.urn);
}

export function projectRawEmote(raw: unknown): Emote | null {
  if (!isRecord(raw)) return null;
  const o = raw as RawEmoteInput;
  const data = o.emoteDataADR74 ?? {};
  const candidate = {
    urn: o.urn ?? o.id,
    name: o.name,
    description: o.description,
    thumbnail: o.thumbnail,
    rarity: o.rarity,
    category: data.category,
    loop: data.loop,
  };
  const row = keepRow(candidate, EmoteSchema, isRenderableEmote);
  return row ? normalizeEmote(row) : null;
}

export function parseOwnedEmotes(raw: unknown): string[] {
  return keepRows(raw, OwnedEmoteElementSchema, hasUrn, (r) => r.urn);
}

/**
 * Called on wire data (`fetchEquipped`) and again on an array this module built
 * (`loadBackpackEmotes`), so it has to be idempotent: schema and guard are both
 * pure predicates over a row, and a binding that passed once passes again.
 */
export function parseLoadout(raw: unknown): SlotBinding[] {
  return keepRows(raw, SlotBindingSchema, isUsableSlotBinding, normalizeSlotBinding);
}

export function findWearable(catalog: Wearable[], urn: string): Wearable | undefined {
  return catalog.find((w) => w.urn === urn);
}

export function byCategory(catalog: Wearable[]): Record<string, Wearable[]> {
  const out: Record<string, Wearable[]> = {};
  for (const w of catalog) {
    (out[w.category] ??= []).push(w);
  }
  return out;
}

export function sortLoadout(loadout: SlotBinding[]): SlotBinding[] {
  const rank = (slot: number) => (slot === 0 ? 10 : slot);
  return [...loadout].sort((a, b) => rank(a.slot) - rank(b.slot));
}

export function rarityLabel(rarity: string): string {
  return rarity.charAt(0).toUpperCase() + rarity.slice(1);
}

function contentUrl(hash: string, base?: string): string {
  return `${catalystBase(base)}/content/contents/${hash}`;
}

function urnNetwork(urn: unknown): string | null {
  if (typeof urn !== "string") return null;
  const m = urn.match(/^urn:decentraland:([a-z-]+):/i);
  const chain = m?.[1]?.toLowerCase();
  if (!chain || chain === "off-chain") return null;
  return chain;
}

function prettyWearableName(urn: unknown, category: string): string {
  let seg = String(urn ?? "").split(":").pop() || "";
  if (category !== "body_shape") seg = seg.replace(/^[fmu]_/i, "");
  return seg
    .replace(/[-_]+/g, " ")
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/([a-zA-Z])(\d)/g, "$1 $2")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function looksLikeRawWearableName(name: unknown): boolean {
  const s = String(name ?? "");
  return s !== "" && !/\s/.test(s) && (/[_-]/.test(s) || s === s.toLowerCase());
}

export function mapExplorerWearable(el: unknown, base?: string): Wearable | null {
  const element = (el ?? {}) as RawElement;
  const ent = element.entity ?? {};
  const md = ent.metadata ?? {};
  const data = md.data ?? {};
  const content: RawContentFile[] = Array.isArray(ent.content) ? ent.content : [];
  const hashOf = (file?: string) => content.find((c) => c.file === file)?.hash;
  const thumbHash = hashOf(md.thumbnail || "thumbnail.png") ?? hashOf(md.image);
  const reps: RawRepresentation[] = Array.isArray(data.representations)
    ? data.representations
    : [];
  const bodyShapes = [
    ...new Set(
      reps.flatMap((r): string[] => (Array.isArray(r?.bodyShapes) ? r.bodyShapes : [])),
    ),
  ];
  const urn = element.urn || md.id || ent.id;
  if (!urn) return null;
  const category = element.category || data.category || "upper_body";
  const rawName = element.name || md.name || "";
  const name =
    !rawName || looksLikeRawWearableName(rawName) || category === "body_shape"
      ? prettyWearableName(urn, category)
      : rawName;
  const candidate = {
    urn,
    name,
    thumbnail: thumbHash ? contentUrl(thumbHash, base) : null,
    rarity: md.rarity || (element.type === "base-wearable" ? "base" : "common"),
    category,
    bodyShapes,
    description: md.description ?? null,
    isSmart:
      Array.isArray(data.requiredPermissions) && data.requiredPermissions.length > 0,
    creator: null,
    network: urnNetwork(urn),
  };
  const row = keepRow(candidate, WearableSchema, isRenderableWearable);
  return row ? normalizeWearable(row) : null;
}

function deriveCategories(catalog: Wearable[]): WearableCategory[] {
  const seen = new Set<string>();
  const out: WearableCategory[] = [];
  for (const w of catalog) {
    if (seen.has(w.category)) continue;
    seen.add(w.category);
    out.push({ id: w.category, label: w.category, slot: w.category });
  }
  return out;
}

function color3ToHex(c: unknown): string | undefined {
  if (!c || typeof c !== "object") return undefined;
  const col = c as { r?: number; g?: number; b?: number };
  const to255 = (n?: number) => Math.max(0, Math.min(255, Math.round((n ?? 0) * 255)));
  const hx = (n: number) => n.toString(16).padStart(2, "0");
  return `#${hx(to255(col.r))}${hx(to255(col.g))}${hx(to255(col.b))}`;
}

export function hexToColor3(hex: unknown): { r: number; g: number; b: number } {
  const fallback = { r: 0, g: 0, b: 0 };
  if (typeof hex !== "string") return fallback;
  let s = hex.trim().replace(/^#/, "");
  if (s.length === 3) s = s.split("").map((ch) => ch + ch).join("");
  if (s.length !== 6) return fallback;
  const n = parseInt(s, 16);
  if (!Number.isFinite(n)) return fallback;
  return {
    r: ((n >> 16) & 255) / 255,
    g: ((n >> 8) & 255) / 255,
    b: (n & 255) / 255,
  };
}

async function fetchEquipped(address?: string | null, opts: RequestOpts = {}): Promise<Equipped> {
  try {
    const addr = normalizeAddress(address);
    if (!addr) return UNKNOWN_EQUIPPED;
    const raw = await getJSON<RawProfileEnv>(
      `/lambdas/profile/${encodeURIComponent(addr)}`,
      opts,
    );
    const av = raw?.avatars?.[0]?.avatar;
    if (!av) return UNKNOWN_EQUIPPED;
    const candidate = {
      bodyShape: av.bodyShape || undefined,
      skinColor: color3ToHex(av.skin?.color),
      hairColor: color3ToHex(av.hair?.color),
      eyeColor: color3ToHex(av.eyes?.color),
      name: raw?.avatars?.[0]?.name || undefined,
      wearables: asStringArray(av.wearables),
      emotes: Array.isArray(av.emotes)
        ? (av.emotes as unknown[])
            .map((e) => (typeof e === "string" ? e : (e as { urn?: string } | null)?.urn))
            .filter(Boolean)
        : [],
      emoteSlots: parseLoadout(av.emotes),
    };
    return normalizeEquipped(EquippedSchema.parse(candidate));
  } catch (err) {
    if (opts.signal?.aborted) throw err;
    return UNKNOWN_EQUIPPED;
  }
}

async function fetchAllExplorerWearables(
  address: string,
  opts: RequestOpts = {},
): Promise<unknown[]> {
  const addr = normalizeAddress(address);
  const pageSize = 1000;
  let pageNum = 1;
  let total = Infinity;
  const all: unknown[] = [];
  while (all.length < total) {
    const raw = await getJSON<{ elements?: unknown[]; totalAmount?: number }>(
      `/lambdas/explorer/${encodeURIComponent(addr)}/wearables`,
      { ...opts, query: { pageSize, pageNum } },
    );
    const els = Array.isArray(raw?.elements) ? raw.elements : [];
    total = Number.isFinite(raw?.totalAmount)
      ? (raw?.totalAmount ?? all.length + els.length)
      : all.length + els.length;
    all.push(...els);
    if (!els.length || els.length < pageSize) break;
    pageNum += 1;
    if (pageNum > 25) break;
  }
  return all;
}

export const BASE_EMOTE_COLLECTION = "urn:decentraland:off-chain:base-emotes";

export const BASE_EMOTE_IDS = [
  "handsair",
  "wave",
  "fistpump",
  "dance",
  "raiseHand",
  "clap",
  "money",
  "kiss",
  "headexplode",
  "shrug",
  "dab",
  "robot",
  "hammer",
  "tik",
  "tektonik",
  "dontsee",
  "disco",
  "snowfall",
  "hohoho",
  "cry",
  "confettipopper",
];

// The classic decentraland emote-wheel default belt (slots 1-9 then 0) --
// used only as a fallback when the profile carries no real per-slot
// assignment (anon/guest, or a legacy profile that never saved one).
export const DEFAULT_EMOTE_BELT = [
  "wave",
  "clap",
  "dance",
  "kiss",
  "headexplode",
  "robot",
  "hammer",
  "tik",
  "snowfall",
  "disco",
];

function baseEmoteUrn(id: string): string {
  return `${BASE_EMOTE_COLLECTION}:${id}`;
}

function prettyEmoteName(id: unknown): string {
  return String(id ?? "")
    .replace(/[-_]+/g, " ")
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/^./, (c) => c.toUpperCase());
}

function i18nName(md: RawMetadata | undefined): string | null {
  const arr = md?.i18n;
  if (!Array.isArray(arr)) return null;
  const list = arr as Array<{ code?: string; text?: string }>;
  return list.find((t) => t?.code === "en")?.text ?? list[0]?.text ?? null;
}

function projectEmoteEntity(
  ent: RawEntity | null | undefined,
  { urn, rarity, base }: { urn?: string; rarity?: string; base?: string } = {},
): Emote | null {
  const md = ent?.metadata ?? {};
  const data = md.emoteDataADR74 ?? {};
  const content: RawContentFile[] = Array.isArray(ent?.content) ? ent.content : [];
  const thumbHash = content.find(
    (c) => c.file === (md.thumbnail || "thumbnail.png"),
  )?.hash;
  const pointers: string[] = Array.isArray(ent?.pointers) ? ent.pointers : [];
  const resolvedUrn = urn || md.id || pointers[0] || ent?.id;
  if (!resolvedUrn) return null;
  return projectRawEmote({
    urn: resolvedUrn,
    name: i18nName(md) || md.name || prettyEmoteName(String(resolvedUrn).split(":").pop()),
    description: md.description,
    thumbnail: thumbHash ? contentUrl(thumbHash, base) : null,
    rarity: rarity || md.rarity,
    emoteDataADR74: { category: data.category, loop: data.loop },
  });
}

export function mapExplorerEmote(el: unknown, base?: string): Emote | null {
  const element = (el ?? {}) as RawElement;
  const e = projectEmoteEntity(element.entity, {
    urn: element.urn || element.entity?.metadata?.id,
    rarity:
      element.entity?.metadata?.rarity || (element.type === "base-emote" ? "base" : "common"),
    base,
  });
  return e;
}

export async function fetchEmoteGlbUrl(
  urn: string,
  opts: RequestOpts = {},
): Promise<string | null> {
  const base = catalystBase(opts.base);
  const res = await (opts.fetchImpl ?? fetch)(`${base}/content/entities/active`, {
    method: "POST",
    signal: opts.signal,
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify({ pointers: [urn] }),
  });
  if (!res.ok) return null;
  const entities: unknown = await res.json();
  const entitiesArr: RawEntity[] = Array.isArray(entities) ? entities : [];
  const ent = entitiesArr[0] ?? null;
  const content: RawContentFile[] = Array.isArray(ent?.content) ? ent.content : [];
  const reps: RawRepresentation[] = ent?.metadata?.emoteDataADR74?.representations ?? [];
  const main = reps[0]?.mainFile;
  const hash =
    (main && content.find((c) => c.file === main)?.hash) ||
    content.find((c) => /\.glb$/i.test(c.file ?? ""))?.hash;
  return hash ? contentUrl(hash, opts.base) : null;
}

async function fetchBaseEmotes(opts: RequestOpts = {}): Promise<Emote[]> {
  const byUrn = new Map<string, Emote>();
  for (const id of BASE_EMOTE_IDS) {
    const urn = baseEmoteUrn(id);
    const e = projectRawEmote({
      urn,
      name: prettyEmoteName(id),
      rarity: "base",
      emoteDataADR74: { category: "miscellaneous", loop: false },
    });
    if (e) byUrn.set(urn.toLowerCase(), e);
  }

  try {
    const base = catalystBase(opts.base);
    const res = await (opts.fetchImpl ?? fetch)(`${base}/content/entities/active`, {
      method: "POST",
      signal: opts.signal,
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({ pointers: BASE_EMOTE_IDS.map(baseEmoteUrn) }),
    });
    if (res.ok) {
      const entities: unknown = await res.json();
      if (Array.isArray(entities)) {
        for (const ent of entities as unknown[]) {
          const e = projectEmoteEntity(ent as RawEntity, { rarity: "base", base: opts.base });
          if (e) byUrn.set(e.urn.toLowerCase(), e);
        }
      }
    }
  } catch (err) {
    if (opts.signal?.aborted) throw err;
  }

  return BASE_EMOTE_IDS.map((id) => byUrn.get(baseEmoteUrn(id).toLowerCase())).filter(
    (e): e is Emote => Boolean(e),
  );
}

async function fetchAllExplorerEmotes(
  address: string,
  opts: RequestOpts = {},
): Promise<unknown[]> {
  const addr = normalizeAddress(address);
  const pageSize = 1000;
  let pageNum = 1;
  let total = Infinity;
  const all: unknown[] = [];
  while (all.length < total) {
    const raw = await getJSON<{ elements?: unknown[]; totalAmount?: number }>(
      `/lambdas/explorer/${encodeURIComponent(addr)}/emotes`,
      {
        ...opts,
        query: { collectionType: "on-chain", pageSize, pageNum },
      },
    );
    const els = Array.isArray(raw?.elements) ? raw.elements : [];
    total = Number.isFinite(raw?.totalAmount)
      ? (raw?.totalAmount ?? all.length + els.length)
      : all.length + els.length;
    all.push(...els);
    if (!els.length || els.length < pageSize) break;
    pageNum += 1;
    if (pageNum > 25) break;
  }
  return all;
}

export async function loadOutfits(address?: string | null, opts: RequestOpts = {}) {
  const addr = normalizeAddress(address);
  if (!isEthAddress(addr)) return [];
  try {
    const env = await getJSON<RawOutfitsEnv>(
      `/lambdas/outfits/${encodeURIComponent(addr)}`,
      opts,
    );
    const list = env?.metadata?.outfits || env?.outfits || [];
    return list
      .map((o) => ({
        slot: Number(o.slot),
        bodyShape: o.outfit?.bodyShape,
        wearables: asStringArray(o.outfit?.wearables),
        skinColor: color3ToHex(o.outfit?.skin?.color),
        hairColor: color3ToHex(o.outfit?.hair?.color),
        eyeColor: color3ToHex(o.outfit?.eyes?.color),
      }))
      .filter((o) => Number.isInteger(o.slot));
  } catch (err) {
    if (opts.signal?.aborted) throw err;
    return [];
  }
}

export async function loadRecentOutfits(count = 4, opts: RequestOpts = {}) {
  try {
    const raw = await getJSON<{ deployments?: RawDeployment[] }>(`/content/deployments`, {
      ...opts,
      query: {
        entityType: "profile",
        limit: 60,
        sortingField: "local_timestamp",
        sortingOrder: "DESC",
      },
    });
    const ds = Array.isArray(raw?.deployments) ? raw.deployments : [];
    const seen = new Set<string>();
    const out: Array<{
      slot: number;
      address: string;
      name: string;
      bodyShape: string | undefined;
      wearables: string[];
      skinColor: string | undefined;
      hairColor: string | undefined;
      eyeColor: string | undefined;
    }> = [];
    for (const d of ds) {
      const av = d?.metadata?.avatars?.[0]?.avatar;
      const addr = (d?.pointers?.[0] || "").toLowerCase();
      const wearables = asStringArray(av?.wearables);
      if (!av || !addr || seen.has(addr) || wearables.length < 4) continue;
      seen.add(addr);
      out.push({
        slot: out.length,
        address: addr,
        name: d?.metadata?.avatars?.[0]?.name || "",
        bodyShape: av.bodyShape,
        wearables,
        skinColor: color3ToHex(av.skin?.color),
        hairColor: color3ToHex(av.hair?.color),
        eyeColor: color3ToHex(av.eyes?.color),
      });
      if (out.length >= count) break;
    }
    return out;
  } catch (err) {
    if (opts.signal?.aborted) throw err;
    return [];
  }
}

export async function loadBackpack(address?: string | null, opts: RequestOpts = {}) {
  const addr = normalizeAddress(address);
  const fetchAddr = isEthAddress(addr)
    ? addr
    : "0x0000000000000000000000000000000000000000";

  const elements = await fetchAllExplorerWearables(fetchAddr, opts);

  const catalog: Wearable[] = [];
  const ownedUrns: string[] = [];
  for (const el of elements) {
    const w = mapExplorerWearable(el, opts.base);
    if (!w) continue;
    catalog.push(w);
    const elem = el as RawElement;
    if (elem.type && elem.type !== "base-wearable") ownedUrns.push(w.urn);
  }

  const categories = deriveCategories(catalog);
  const equipped = await fetchEquipped(addr, opts);

  const ownedSet = new Set(ownedUrns);
  const owned = catalog.filter((w) => ownedSet.has(w.urn));

  return {
    address: addr,
    owned,
    ownedUrns,
    catalog,
    categories,
    equipped,
    ownedEmpty: owned.length === 0,
    source: "live",
  };
}

export async function loadBackpackEmotes(address?: string | null, opts: RequestOpts = {}) {
  const addr = normalizeAddress(address);

  const baseEmotes = await fetchBaseEmotes(opts);

  let ownedEmotes: Emote[] = [];
  let equippedSlots: SlotBinding[] | null = null;
  if (isEthAddress(addr)) {
    try {
      const [els, equipped] = await Promise.all([
        fetchAllExplorerEmotes(addr, opts),
        fetchEquipped(addr, opts),
      ]);
      equippedSlots = equipped.emoteSlots;
      const seen = new Set<string>();
      for (const el of els) {
        const e = mapExplorerEmote(el, opts.base);
        if (!e || seen.has(e.urn)) continue;
        seen.add(e.urn);
        ownedEmotes.push(e);
      }
    } catch (err) {
      if (opts.signal?.aborted) throw err;
      ownedEmotes = [];
    }
  }

  const catalog: Emote[] = [];
  const inCatalog = new Set<string>();
  for (const e of [...baseEmotes, ...ownedEmotes]) {
    if (inCatalog.has(e.urn)) continue;
    inCatalog.add(e.urn);
    catalog.push(e);
  }

  const ownedUrns = ownedEmotes.map((e) => e.urn);

  // Real per-slot equip data from the profile wins when present; otherwise
  // fall back to the classic default belt so a fresh/guest wheel still
  // shows something playable.
  const catalogByUrn = new Map(catalog.map((e) => [e.urn, e]));
  const rawLoadout = equippedSlots?.length
    ? equippedSlots.map((b) => ({
        slot: b.slot,
        urn: b.urn,
        name: catalogByUrn.get(b.urn)?.name ?? b.name,
      }))
    : DEFAULT_EMOTE_BELT.map((id, i) => {
        const urn = baseEmoteUrn(id);
        return {
          slot: SLOT_ORDER[i],
          urn,
          name: catalogByUrn.get(urn)?.name ?? prettyEmoteName(id),
        };
      });
  const loadout = sortLoadout(parseLoadout(rawLoadout));

  return {
    address: addr || "anon",
    catalog,
    owned: ownedEmotes,
    ownedUrns,
    loadout,
    slotOrder: [...SLOT_ORDER],
    liveEmpty: ownedUrns.length === 0,
    source: "live",
  };
}

export type OutfitInput = {
  slot: number;
  bodyShape?: string;
  wearables?: string[];
  skinColor?: string;
  hairColor?: string;
  eyeColor?: string;
};

export const MAX_BASE_OUTFIT_SLOTS = 5;

export function buildOutfitsMetadata(outfits: OutfitInput[]) {
  const seen = new Set<number>();
  const valid = (outfits ?? []).filter((o) => {
    const ok =
      Number.isInteger(o.slot) &&
      o.slot >= 0 &&
      o.slot < MAX_BASE_OUTFIT_SLOTS &&
      !!o.bodyShape &&
      !seen.has(o.slot);
    if (ok) seen.add(o.slot);
    return ok;
  });
  return {
    outfits: valid.map((o) => ({
      slot: o.slot,
      outfit: {
        bodyShape: o.bodyShape,
        eyes: { color: hexToColor3(o.eyeColor) },
        hair: { color: hexToColor3(o.hairColor) },
        skin: { color: hexToColor3(o.skinColor) },
        wearables: o.wearables ?? [],
        forceRender: [] as string[],
      },
    })),
    namesForExtraSlots: [] as string[],
  };
}

export type SaveOutfitsResult =
  | { ok: true; entityId: string }
  | { ok: false; reason: string };

const RFC4648_BASE32_LOWER = "abcdefghijklmnopqrstuvwxyz234567";

function multibaseBase32Lower(data: Uint8Array): string {
  let bits = 0;
  let value = 0;
  let out = "";
  for (let i = 0; i < data.length; i++) {
    value = (value << 8) | data[i]!;
    bits += 8;
    while (bits >= 5) {
      out += RFC4648_BASE32_LOWER.charAt((value >>> (bits - 5)) & 31);
      bits -= 5;
    }
  }
  if (bits > 0) out += RFC4648_BASE32_LOWER.charAt((value << (5 - bits)) & 31);
  return `b${out}`;
}

function pushVarint(buf: number[], v: number): void {
  let value = v;
  for (;;) {
    const byte = value & 0x7f;
    value >>>= 7;
    if (value === 0) {
      buf.push(byte);
      break;
    }
    buf.push(byte | 0x80);
  }
}

// Catalyst entity id: CIDv1, raw codec (0x55), sha256 (0x12) multihash, base32-lower
// multibase -- the same construction the server deploy path uses, but hashed with
// SubtleCrypto so it runs in the browser bundle.
async function computeEntityId(bytes: Uint8Array): Promise<string> {
  const digest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", bytes as BufferSource),
  );
  const cid: number[] = [];
  pushVarint(cid, 1);
  pushVarint(cid, 0x55);
  cid.push(0x12, 0x20);
  for (const b of digest) cid.push(b);
  return multibaseBase32Lower(Uint8Array.from(cid));
}

// Deploys the user's own signed outfits entity to /content/entities. Signing uses
// the wallet-delegated ephemeral identity already persisted by the engine login
// (the same key that authorizes signed-fetch writes), appending an
// ECDSA_SIGNED_ENTITY link over the entity id. Fail-closed: with no valid stored
// identity or non-eth address the deploy is skipped and a reason is returned --
// never a silent fake success.
export async function saveOutfits(
  address: string | null | undefined,
  outfits: OutfitInput[],
  opts: RequestOpts = {},
): Promise<SaveOutfitsResult> {
  const addr = normalizeAddress(address);
  if (!isEthAddress(addr)) {
    return { ok: false, reason: "connect a wallet to save outfits" };
  }

  const identity = loadStoredIdentity();
  if (!identity) {
    return { ok: false, reason: "sign in with a wallet to save outfits" };
  }

  const deployment = {
    version: "v3",
    type: "outfits",
    pointers: [`${addr}:outfits`],
    timestamp: Date.now(),
    content: [] as unknown[],
    metadata: buildOutfitsMetadata(outfits),
  };
  const entityBytes = new TextEncoder().encode(JSON.stringify(deployment));
  const entityId = await computeEntityId(entityBytes);

  const account = privateKeyToAccount(
    identity.ephemeralIdentity.privateKey as `0x${string}`,
  );
  const signature = await account.signMessage({ message: entityId });
  const authChain: AuthLink[] = [
    ...identity.authChain,
    { type: "ECDSA_SIGNED_ENTITY", payload: entityId, signature },
  ];

  const form = new FormData();
  form.set("entityId", entityId);
  authChain.forEach((link, i) => {
    form.set(`authChain[${i}][type]`, link.type);
    form.set(`authChain[${i}][payload]`, link.payload);
    form.set(`authChain[${i}][signature]`, link.signature ?? "");
  });
  form.set(
    entityId,
    new Blob([entityBytes], { type: "application/octet-stream" }),
    entityId,
  );

  const base = catalystBase(opts.base);
  let res: Response;
  try {
    res = await (opts.fetchImpl ?? fetch)(`${base}/content/entities`, {
      method: "POST",
      body: form,
      signal: opts.signal,
    });
  } catch (err) {
    if (opts.signal?.aborted) throw err;
    return { ok: false, reason: `outfits deploy request failed: ${String(err)}` };
  }

  if (!res.ok) {
    let detail = "";
    try {
      detail = await res.text();
    } catch {
    }
    return {
      ok: false,
      reason: `outfits deploy failed: ${res.status} ${res.statusText}${detail ? `: ${detail}` : ""}`,
    };
  }
  return { ok: true, entityId };
}
