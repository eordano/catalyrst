import * as THREE from "three";

interface ColorWrapper {
  color?: { r: number; g: number; b: number };
}

export interface OutfitData {
  address?: string;
  slot?: number;
  bodyShape?: string;
  wearables?: string[];
  skin?: ColorWrapper;
  hair?: ColorWrapper;
  eyes?: ColorWrapper;
}

export type AvatarColors = {
  skin: THREE.Color | null;
  hair: THREE.Color | null;
  eyes: THREE.Color | null;
};

interface Representation {
  bodyShapes?: string[];
  mainFile?: string;
  contents?: string[];
}

interface EntityData {
  category?: string;
  representations?: Representation[];
  hides?: string[];
  replaces?: string[];
  removesDefaultHiding?: string[];
}

export interface Entity {
  pointers?: string[];
  content?: { file: string; hash: string }[];
  metadata?: { data?: EntityData };
}

interface Avatar {
  bodyShape?: string;
  wearables?: string[];
  skin?: ColorWrapper;
  hair?: ColorWrapper;
  eyes?: ColorWrapper;
}

interface ProfileEnvelope {
  avatars?: { avatar?: Avatar }[];
}

interface OutfitSlot {
  slot?: number;
  outfit?: OutfitData;
}

interface OutfitsEnvelope {
  metadata?: { outfits?: OutfitSlot[] };
  outfits?: OutfitSlot[];
}

interface OutfitSource {
  body?: string;
  urns?: string[] | string;
  outfit?: OutfitData | null;
  profile?: string;
}

export interface ResolvedOutfit {
  bodyShape: string;
  wearables: string[];
  colors: AvatarColors;
}

export interface HidingRules {
  hidden: Set<string>;
  equippedCats: Set<string>;
  skinEquipped: boolean;
  handsDefaultHidden: boolean;
}

const DEFAULT_BODY = "urn:decentraland:off-chain:base-avatars:BaseMale";
export const FACIAL_CATS = new Set(["eyes", "eyebrows", "mouth"]);

const itemUrn = (urn: string): string => {
  const p = urn.split(":");
  return p.length === 7 && p[3] !== undefined && /^collections-v[12]$/.test(p[3])
    ? p.slice(0, 6).join(":")
    : urn;
};

const colorOf = (c: ColorWrapper | null | undefined): THREE.Color | null =>
  c && c.color && typeof c.color.r === "number"
    ? new THREE.Color(c.color.r, c.color.g, c.color.b)
    : null;

export const categoryOf = (e: Entity | undefined): string | null =>
  e?.metadata?.data?.category || null;

async function getJSON<T>(url: string, opts?: RequestInit): Promise<T> {
  const r = await fetch(url, opts);
  if (!r.ok) throw new Error(`${url} -> ${r.status}`);
  return r.json() as Promise<T>;
}

const entityCache = new Map<string, Promise<Entity | null>>();

export function fetchEntities(base: string, pointers: string[]): Promise<Map<string, Entity>> {
  const wanted = [...new Set(pointers.map((p) => p.toLowerCase()))];
  const missing = wanted.filter((p) => !entityCache.has(`${base}|${p}`));
  if (missing.length) {
    const batch = getJSON<Entity[]>(`${base}/content/entities/active`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ pointers: missing }),
    });
    for (const p of missing) {
      const key = `${base}|${p}`;
      entityCache.set(
        key,
        batch
          .then(
            (list) =>
              list.find((e) => (e.pointers || []).some((x) => String(x).toLowerCase() === p)) ??
              null,
          )
          .catch((err) => {
            entityCache.delete(key);
            throw err;
          }),
      );
    }
  }
  return Promise.all(
    wanted.map(async (p) => [p, await entityCache.get(`${base}|${p}`)] as const),
  ).then((pairs) => {
    const map = new Map<string, Entity>();
    for (const [p, e] of pairs) if (e) map.set(p, e);
    return map;
  });
}

export function representationMainFile(entity: Entity, bodyShape: string): string | null {
  const reps = entity?.metadata?.data?.representations || [];
  const bs = bodyShape.toLowerCase();
  const rep =
    reps.find((r) => (r.bodyShapes || []).some((b) => String(b).toLowerCase() === bs)) || reps[0];
  return rep?.mainFile || null;
}

export function representationContents(entity: Entity, bodyShape: string): string[] {
  const reps = entity?.metadata?.data?.representations || [];
  const bs = bodyShape.toLowerCase();
  const rep =
    reps.find((r) => (r.bodyShapes || []).some((b) => String(b).toLowerCase() === bs)) || reps[0];
  return (rep?.contents?.length ? rep.contents : (entity.content || []).map((c) => c.file)).map(
    (n) => String(n).toLowerCase(),
  );
}

export function fileMapFor(entity: Entity): Map<string, string> {
  const map = new Map<string, string>();
  for (const c of entity.content || []) {
    const f = String(c.file).toLowerCase();
    map.set(f, c.hash);
    const last = f.split("/").pop();
    if (last !== undefined) map.set(last, c.hash);
  }
  return map;
}

export async function resolveOutfit(base: string, src: OutfitSource): Promise<ResolvedOutfit> {
  let bodyShape = src.body || DEFAULT_BODY;
  let wearables = Array.isArray(src.urns)
    ? src.urns.slice()
    : String(src.urns || "")
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
  let colors: AvatarColors = { skin: null, hair: null, eyes: null };

  let outfit: OutfitData | null = src.outfit && typeof src.outfit === "object" ? src.outfit : null;
  if (outfit && outfit.address) {
    try {
      const env = await getJSON<OutfitsEnvelope>(
        `${base}/lambdas/outfits/${String(outfit.address).toLowerCase()}`,
      );
      const list = env?.metadata?.outfits || env?.outfits || [];
      const slot = outfit.slot ?? 0;
      outfit = (list.find((o) => o.slot === slot) || list[0])?.outfit || null;
    } catch {
      outfit = null;
    }
  }

  if (outfit && (Array.isArray(outfit.wearables) || outfit.bodyShape)) {
    if (outfit.bodyShape) bodyShape = outfit.bodyShape;
    if (Array.isArray(outfit.wearables)) wearables = outfit.wearables;
    colors = { skin: colorOf(outfit.skin), hair: colorOf(outfit.hair), eyes: colorOf(outfit.eyes) };
  } else if (src.profile) {
    const env = await getJSON<ProfileEnvelope>(
      `${base}/lambdas/profile/${String(src.profile).toLowerCase()}`,
    );
    const av: Avatar = (env.avatars || [])[0]?.avatar || {};
    if (av.bodyShape) bodyShape = av.bodyShape;
    if (Array.isArray(av.wearables) && av.wearables.length) wearables = av.wearables;
    colors = { skin: colorOf(av.skin), hair: colorOf(av.hair), eyes: colorOf(av.eyes) };
  }

  const seen = new Set<string>();
  wearables = wearables.map(itemUrn).filter((u) => (seen.has(u) ? false : (seen.add(u), true)));
  return { bodyShape, wearables, colors };
}

const SKIN_HIDES = [
  "eyes", "mouth", "eyebrows", "hair", "facial_hair",
  "upper_body", "lower_body", "feet", "hands_wear", "hands", "head",
];

export function computeHiding(wearables: string[], byPointer: Map<string, Entity>): HidingRules {
  const equippedCats = new Set<string>();
  const hidden = new Set<string>();
  let skinEquipped = false;
  let handsDefaultHidden = false;
  for (const urn of wearables) {
    const e = byPointer.get(urn.toLowerCase());
    const cat = categoryOf(e);
    if (!e || !cat) continue;
    equippedCats.add(cat);
    if (cat === "skin") skinEquipped = true;
    const d: EntityData = e.metadata?.data || {};
    for (const h of [...(d.hides || []), ...(d.replaces || [])]) if (h !== cat) hidden.add(h);
    const coversUpperBody = cat === "upper_body" || (d.hides || []).includes("upper_body");
    if (coversUpperBody && !(d.removesDefaultHiding || []).includes("hands"))
      handsDefaultHidden = true;
  }
  if (skinEquipped) for (const c of SKIN_HIDES) hidden.add(c);
  return { hidden, equippedCats, skinEquipped, handsDefaultHidden };
}

const BASE_MESH_HIDERS: [string, (r: HidingRules) => boolean][] = [
  ["ubody_basemesh", (r) => r.equippedCats.has("upper_body") || r.hidden.has("upper_body")],
  ["lbody_basemesh", (r) => r.equippedCats.has("lower_body") || r.hidden.has("lower_body")],
  ["feet_basemesh", (r) => r.equippedCats.has("feet") || r.hidden.has("feet")],
  [
    "hands_basemesh",
    (r) =>
      r.equippedCats.has("hands_wear") ||
      r.hidden.has("hands") ||
      r.hidden.has("hands_wear") ||
      r.handsDefaultHidden,
  ],
  ["head_basemesh", (r) => r.hidden.has("head")],
  ["mask_eyes", (r) => r.hidden.has("eyes")],
  ["mask_eyebrows", (r) => r.hidden.has("eyebrows")],
  ["mask_mouth", (r) => r.hidden.has("mouth")],
];

export function baseMeshHidden(names: string[], rules: HidingRules): boolean {
  return BASE_MESH_HIDERS.some(
    ([suffix, pred]) =>
      names.some((n) => n.endsWith(suffix)) && (rules.skinEquipped || pred(rules)),
  );
}
