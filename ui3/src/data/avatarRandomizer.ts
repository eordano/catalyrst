export type WearableCatalog = Record<string, string[]>;

export type WearableCatalogs = { male: WearableCatalog; female: WearableCatalog };

export type WearableCatalogEntry = {
  urn?: string;
  category?: string;
  bodyShapes?: unknown;
};

const FEMALE_EXCLUDED_CATEGORIES = new Set<string>(["facial_hair"]);

const OPTIONAL_CATEGORIES = new Set<string>([
  "facial_hair",
  "hat",
  "mask",
  "tiara",
  "helmet",
  "earring",
  "eyewear",
  "top_head",
  "hands_wear",
]);

const OPTIONAL_CATEGORY_INCLUDE_CHANCE = 0.75;

function lastSegment(urn: string): string {
  const i = urn.lastIndexOf(":");
  return i >= 0 ? urn.slice(i + 1) : urn;
}

function hasBodyTypePrefix(urn: string, prefix: string): boolean {
  return lastSegment(String(urn)).toLowerCase().startsWith(prefix);
}

function isCompatible(bodyShapes: unknown, needle: string): boolean {
  const list: unknown[] = Array.isArray(bodyShapes) ? bodyShapes : [];
  return list.some((s) => String(s).includes(needle));
}

export function buildWearableCatalogs(
  catalog: readonly WearableCatalogEntry[] | null | undefined,
): WearableCatalogs {
  const male: WearableCatalog = {};
  const female: WearableCatalog = {};
  const add = (cat: string, c: WearableCatalog, urn: string) => {
    (c[cat] || (c[cat] = [])).push(urn);
  };

  for (const w of Array.isArray(catalog) ? catalog : []) {
    const category = w?.category;
    const urn = w?.urn;
    if (!category || !urn || category === "body_shape") continue;

    if (isCompatible(w.bodyShapes, "BaseMale") && !hasBodyTypePrefix(urn, "f_"))
      add(category, male, urn);

    if (
      isCompatible(w.bodyShapes, "BaseFemale") &&
      !FEMALE_EXCLUDED_CATEGORIES.has(category) &&
      !hasBodyTypePrefix(urn, "m_")
    )
      add(category, female, urn);
  }

  return { male, female };
}

function pick<T>(arr: readonly T[]): T {
  const value = arr[Math.floor(Math.random() * arr.length)];
  if (value === undefined) throw new Error("pick(): empty array");
  return value;
}

export function selectRandomWearables(
  catalogs: WearableCatalogs | null | undefined,
  body?: string,
): string[] {
  const catalog = body === "B" ? catalogs?.female : catalogs?.male;
  if (!catalog) return [];
  const out: string[] = [];
  for (const category of Object.keys(catalog)) {
    const list = catalog[category];
    if (!list || list.length === 0) continue;
    if (
      OPTIONAL_CATEGORIES.has(category) &&
      Math.random() > OPTIONAL_CATEGORY_INCLUDE_CHANCE
    )
      continue;
    out.push(pick(list));
  }
  return out;
}
