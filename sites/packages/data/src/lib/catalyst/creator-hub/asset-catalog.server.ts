import { z } from "zod";

import { getJSON, catalystBase } from "../client";
import { track } from "@core/lib/telemetry/track";

export type CatalogItem = {
  id: string;
  name: string;
  category: string;
  pack: string;
  thumbnailUrl: string;
  glbFile: string;
  glbUrl: string;
  contents?: Record<string, string>;
  isMultiFile?: boolean;
  composite?: unknown;
  smart?: boolean;
};

const BuilderAssetSchema = z.object({
  id: z.string().nullish(),
  name: z.string().nullish(),
  category: z.string().nullish(),
  thumbnail: z.string().nullish(),
  model: z.string().nullish(),
  script: z.string().nullish(),
  contents: z.record(z.string(), z.string()).nullish(),
  composite: z.unknown().nullish(),
});
const BuilderPackSchema = z.object({
  title: z.string().nullish(),
  assets: z.array(BuilderAssetSchema).nullish(),
});
const AssetPacksResponseSchema = z.object({
  data: z.array(BuilderPackSchema).nullish(),
});
type AssetPacksResponse = z.infer<typeof AssetPacksResponseSchema>;

const BUILDER_ITEMS_PREFIX = "/builder-items/";

const CACHE_TTL_MS = 60 * 60 * 1000;
let cache: { at: number; items: CatalogItem[] } | null = null;

function resolveHash(
  contents: Record<string, string> | null | undefined,
  ref: string | null | undefined,
): string | undefined {
  if (!ref) return undefined;
  if (contents && typeof contents[ref] === "string" && contents[ref]) return contents[ref];
  return ref;
}

function flatten(resp: AssetPacksResponse): CatalogItem[] {
  const out: CatalogItem[] = [];
  const seen = new Set<string>();
  for (const pack of resp.data ?? []) {
    const packTitle = (pack.title ?? "").trim() || "Assets";
    for (const asset of pack.assets ?? []) {
      const id = (asset.id ?? "").trim();
      const name = (asset.name ?? "").trim();
      const model = (asset.model ?? "").trim();
      if (!id || !name || !model) continue;
      const glbHash = resolveHash(asset.contents, model);
      if (!glbHash) continue;
      const thumbHash = resolveHash(asset.contents, asset.thumbnail);
      if (seen.has(id)) continue;
      seen.add(id);
      const contents = asset.contents ?? undefined;
      const fileCount = contents ? Object.keys(contents).length : 0;
      const isMultiFile = /\.gltf$/i.test(model) || fileCount > 2;
      const smart = (asset.script ?? "").trim() !== "";
      out.push({
        id,
        name,
        category: (asset.category ?? "").trim() || packTitle,
        pack: packTitle,
        thumbnailUrl: thumbHash ? `${BUILDER_ITEMS_PREFIX}${thumbHash}` : "",
        glbFile: model,
        glbUrl: `${BUILDER_ITEMS_PREFIX}${glbHash}`,
        contents,
        isMultiFile,
        composite: asset.composite ?? undefined,
        ...(smart ? { smart: true } : {}),
      });
    }
  }
  return out;
}

export type LoadCatalogOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
};

/**
 * null when the builder asset packs could not be read.
 *
 * `[]` meant "this deployment ships no smart items", which the editor draws as
 * an empty asset palette -- the same picture a creator gets when the builder API
 * is simply down. The caller falls back to the bundled seed catalog on null
 * instead of showing a browser with nothing in it.
 */
export async function loadAssetCatalog(
  opts: LoadCatalogOptions = {},
): Promise<CatalogItem[] | null> {
  if (!opts.base && cache && Date.now() - cache.at < CACHE_TTL_MS) {
    return cache.items;
  }
  try {
    const raw = await getJSON<unknown>("/builder-api/v1/assetPacks", {
      base: catalystBase(opts.base),
      signal: opts.signal,
      fetchImpl: opts.fetchImpl,
      cache: "force-cache",
    });
    const parsed = AssetPacksResponseSchema.safeParse(raw);
    if (!parsed.success) {
      console.warn("[catalyst] assetPacks failed schema validation", parsed.error.issues);
      track(
        "catalyst_schema_drift",
        {
          module: "creator-hub/asset-catalog",
          path: "/builder-api/v1/assetPacks",
          issues: parsed.error.issues
            .slice(0, 5)
            .map((i) => `${i.path.map(String).join(".")}:${i.code}`),
        },
        { sid: "schema-drift" },
      );
    }
    const items = flatten(parsed.success ? parsed.data : (raw as AssetPacksResponse));
    if (!opts.base) cache = { at: Date.now(), items };
    return items;
  } catch {
    return null;
  }
}
