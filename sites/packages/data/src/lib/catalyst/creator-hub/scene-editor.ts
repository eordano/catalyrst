import { z } from "zod";

import { catalystBase } from "../client";
import { track } from "@core/lib/telemetry/track";
import editorDefaults from "./scene-editor-defaults.data.json";

export const Vector3Schema = z.object({
  x: z.number(),
  y: z.number(),
  z: z.number(),
});
export type Vector3 = z.infer<typeof Vector3Schema>;

export const QuaternionSchema = z.object({
  x: z.number(),
  y: z.number(),
  z: z.number(),
  w: z.number(),
});

export const TransformSchema = z.object({
  position: Vector3Schema,
  rotation: QuaternionSchema,
  scale: Vector3Schema,
});
export type Transform = z.infer<typeof TransformSchema>;

/** The editor's own catalog shape. `deriveAssetCatalog` and the bundled
 *  `scene-editor-defaults.data.json` are the only producers, and both compute
 *  a hue per asset, so there is nothing for a default to stand in for. */
export const AssetSchema = z.object({
  id: z.string(),
  name: z.string(),
  pack: z.string(),
  src: z.string(),
  hue: z.number(),
  category: z.string().optional(),
  smart: z.boolean().optional(),
});
export type Asset = z.infer<typeof AssetSchema>;

export const AssetCatalogSchema = z.object({
  categories: z.array(z.string()),
  models: z.array(AssetSchema),
});
export type AssetCatalog = z.infer<typeof AssetCatalogSchema>;

export const ComponentDefSchema = z.object({
  id: z.number(),
  key: z.string(),
  label: z.string(),
  componentName: z.string(),
  fields: z.array(z.string()),
});
export type ComponentDef = z.infer<typeof ComponentDefSchema>;

/** `parent: 0` is the scene root, so a defaulted parent silently reparented an
 *  entity to the root; `components: []` said an entity carried none. Both are
 *  computed by `deriveHierarchy` for every node it emits. */
export const HierarchyNodeSchema = z.object({
  entity: z.number(),
  name: z.string(),
  parent: z.number(),
  selected: z.boolean(),
  components: z.array(z.string()),
});
export type HierarchyNode = z.infer<typeof HierarchyNodeSchema>;

/** `parcels`, `contentCount` and `live` are the three facts the editor header
 *  states about a scene -- how big it is, how many files it holds, and whether
 *  it is deployed. Every producer computes them from an active-entity read. */
export const SceneInfoSchema = z.object({
  pointer: z.string(),
  title: z.string(),
  base: z.string(),
  parcels: z.array(z.string()),
  contentCount: z.number(),
  live: z.boolean(),
  template: z.string().optional(),
});
export type SceneInfo = z.infer<typeof SceneInfoSchema>;

export type SceneEditorSeed = {
  scene: SceneInfo;
  transformIdentity: Transform;
  hierarchy: HierarchyNode[];
  assetCatalog: AssetCatalog;
  components: ComponentDef[];
};

const FixtureSchema = z.object({
  scene: z.object({
    pointer: z.string(),
    title: z.string(),
    base: z.string(),
    parcels: z.array(z.string()),
  }),
  transformIdentity: TransformSchema,
  hierarchy: z.array(HierarchyNodeSchema).min(1),
  assetCatalog: AssetCatalogSchema,
  components: z.array(ComponentDefSchema).min(1),
});

export function emptySeed(pointer = "0,0"): SceneEditorSeed {
  const cfg = FixtureSchema.parse(editorDefaults);
  return {
    scene: {
      pointer,
      title: "Untitled Scene",
      base: pointer,
      parcels: [pointer],
      contentCount: 0,
      live: false,
    },
    transformIdentity: cfg.transformIdentity,
    hierarchy: [{ entity: 0, name: "Scene", parent: 0, selected: false, components: [] }],
    assetCatalog: cfg.assetCatalog,
    components: cfg.components,
  };
}

export function layoutToParcels(layout?: string | null): string[] {
  const m = /^(\d+)\s*x\s*(\d+)$/i.exec((layout ?? "").trim());
  if (!m) return ["0,0"];
  const cols = Math.min(Math.max(parseInt(m[1], 10) || 1, 1), 8);
  const rows = Math.min(Math.max(parseInt(m[2], 10) || 1, 1), 8);
  const out: string[] = [];
  for (let y = 0; y < rows; y += 1)
    for (let x = 0; x < cols; x += 1) out.push(`${x},${y}`);
  return out;
}

export function newSceneSeed(
  opts: { name?: string; template?: string; layout?: string | null; parcels?: string[] } = {},
): SceneEditorSeed {
  const parcels =
    opts.parcels && opts.parcels.length ? opts.parcels : layoutToParcels(opts.layout);
  const base = parcels[0] ?? "0,0";
  const name = (opts.name ?? "").trim() || "Untitled Scene";
  const template = (opts.template ?? "").trim();
  const seed = emptySeed(base);
  seed.scene = {
    ...seed.scene,
    title: name,
    base,
    parcels,
    contentCount: 0,
    live: false,
    ...(template && template !== "empty" ? { template } : {}),
  };
  seed.hierarchy = [{ entity: 0, name, parent: 0, selected: false, components: [] }];
  if (template && template !== "empty") {
    seed.hierarchy.push({
      entity: 512,
      name: "Spawn Point",
      parent: 0,
      selected: false,
      components: ["core::Transform"],
    });
  }
  return seed;
}

/** `pointers` and `content` are required on a catalyst entity. `content: []`
 *  was the sharp one: it becomes `contentCount`, and it is also the list the
 *  loader searches for `main.composite`, so a truncated entity opened as a
 *  deployed-but-empty scene instead of failing the read. */
const ActiveEntitySchema = z.object({
  type: z.string(),
  pointers: z.array(z.string()),
  content: z.array(z.object({ file: z.string(), hash: z.string() })),
  metadata: z
    .object({
      display: z.object({ title: z.string().optional() }).optional(),
      scene: z.object({ base: z.string().optional() }).optional(),
    })
    .optional(),
});

export type LoadSeedOptions = {
  base?: string;
  signal?: AbortSignal;
  fetchImpl?: typeof fetch;
  pointer?: string;
};

const DERIVED_ASSET_CAP = 64;

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null;

const unwrap = (raw: unknown): unknown =>
  isObj(raw) && "json" in raw ? (raw as { json: unknown }).json : raw;

/** A composite with no `components` key is not a composite: every entity, name
 *  and transform in the scene lives in that array, so defaulting it to `[]`
 *  opened an empty editor over a scene whose contents we failed to read. */
const CompositeSchema = z.object({
  version: z.number().optional(),
  components: z.array(
    z.object({
      name: z.string(),
      data: z.record(z.string(), z.unknown()),
    }),
  ),
});

const NameV = z.object({
  value: z.string().optional(),
  name: z.string().optional(),
});
const TransV = z.object({ parent: z.number().optional() }).passthrough();
const SrcV = z.object({ src: z.string() });

type CompMap = Map<string, Record<string, unknown>>;

const GLB_RE = /\.(glb|gltf)$/i;

function dedupe(items: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const it of items) {
    if (seen.has(it)) continue;
    seen.add(it);
    out.push(it);
  }
  return out;
}

function basename(p: string): string {
  return (p.split("/").pop() ?? p).replace(GLB_RE, "");
}

function slugify(s: string): string {
  return (
    s
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "asset"
  );
}

function humanize(s: string): string {
  return (
    s
      .replace(/[_-]+/g, " ")
      .replace(/\s+/g, " ")
      .trim()
      .replace(/\b\w/g, (c) => c.toUpperCase()) || "Asset"
  );
}

function stableHue(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i += 1) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  return h % 360;
}

function packFor(p: string): string {
  const parts = p.split("/").filter(Boolean);
  const top = parts[0] === "assets" ? parts[1] : parts[0];
  if (top === "asset-packs") return "Smart Items";
  if (top === "models") return "Models";
  return "Scene";
}

function rootChildOrder(nodesData?: Record<string, unknown>): number[] {
  if (!nodesData) return [];
  const NodesV = z.object({
    value: z.array(
      z.object({ entity: z.number(), children: z.array(z.number()) }).passthrough(),
    ),
  });
  const parsed = NodesV.safeParse(unwrap(nodesData["0"]));
  if (!parsed.success) return [];
  return parsed.data.value.find((r) => r.entity === 0)?.children ?? [];
}

function sceneRootName(comps: CompMap, fallback: string): string {
  const data = comps.get("inspector::SceneMetadata-v3");
  if (data) {
    const p = z.object({ name: z.string() }).safeParse(unwrap(data["0"]));
    if (p.success && p.data.name) return p.data.name;
  }
  return fallback || "Scene";
}

function deriveHierarchy(comps: CompMap, sceneTitle: string): HierarchyNode[] {
  const compsForId = new Map<number, Set<string>>();
  for (const [name, data] of comps) {
    for (const key of Object.keys(data)) {
      const n = Number(key);
      if (!Number.isFinite(n)) continue;
      let set = compsForId.get(n);
      if (!set) {
        set = new Set();
        compsForId.set(n, set);
      }
      set.add(name);
    }
  }
  if (!compsForId.has(0)) compsForId.set(0, new Set());

  const nameData = comps.get("core-schema::Name") ?? {};
  const transData = comps.get("core::Transform") ?? {};
  const selData = comps.get("inspector::Selection") ?? {};
  const order = rootChildOrder(comps.get("inspector::Nodes"));
  const rootName = sceneRootName(comps, sceneTitle);

  const nodes: HierarchyNode[] = [];
  for (const id of compsForId.keys()) {
    const key = String(id);

    let name: string;
    const np = NameV.safeParse(unwrap(nameData[key]));
    if (np.success && (np.data.value || np.data.name)) {
      name = (np.data.value ?? np.data.name) as string;
    } else if (id === 0) {
      name = rootName;
    } else {
      name = `Entity ${id}`;
    }

    let parent = 0;
    if (id !== 0) {
      const tp = TransV.safeParse(unwrap(transData[key]));
      if (tp.success && typeof tp.data.parent === "number" && Number.isFinite(tp.data.parent)) {
        parent = tp.data.parent;
      }
    }

    const components = Array.from(compsForId.get(id) ?? []).sort();
    const selected = Object.prototype.hasOwnProperty.call(selData, key);
    nodes.push({ entity: id, name, parent, selected, components });
  }

  nodes.sort((a, b) => {
    if (a.entity === 0) return -1;
    if (b.entity === 0) return 1;
    const ia = order.indexOf(a.entity);
    const ib = order.indexOf(b.entity);
    if (ia !== -1 && ib !== -1) return ia - ib;
    if (ia !== -1) return -1;
    if (ib !== -1) return 1;
    return a.entity - b.entity;
  });

  const ok = z.array(HierarchyNodeSchema).safeParse(nodes);
  return ok.success ? ok.data : [];
}

function placeholderSrcs(comps: CompMap): string[] {
  const out: string[] = [];
  const data = comps.get("asset-packs::Placeholder");
  if (!data) return out;
  for (const raw of Object.values(data)) {
    const p = SrcV.safeParse(unwrap(raw));
    if (p.success && GLB_RE.test(p.data.src)) out.push(p.data.src);
  }
  return out;
}

function deriveAssetCatalog(bundled: AssetCatalog, glbPaths: string[]): AssetCatalog {
  const derived: Asset[] = [];
  const seenIds = new Set<string>();
  for (const p of glbPaths) {
    const base = basename(p);
    if (!base) continue;
    let id = slugify(base);
    while (seenIds.has(id)) id = `${id}-${derived.length}`;
    seenIds.add(id);
    derived.push({ id, name: humanize(base), pack: packFor(p), src: p, hue: stableHue(base) });
  }
  if (derived.length === 0) return bundled;

  const merged = {
    categories: Array.from(
      new Set([...derived.map((m) => m.pack), ...bundled.categories]),
    ),
    models: [...derived, ...bundled.models],
  };
  const ok = AssetCatalogSchema.safeParse(merged);
  return ok.success ? ok.data : bundled;
}

function deriveComponents(
  bundled: ComponentDef[],
  configData: Record<string, unknown>,
): ComponentDef[] {
  const out = [...bundled];
  const seen = new Set(out.map((c) => c.componentName));
  const ConfigV = z.object({
    componentName: z.string(),
    fields: z.array(z.object({ name: z.string() }).passthrough()),
  });
  let idx = 0;
  for (const raw of Object.values(configData)) {
    const p = ConfigV.safeParse(unwrap(raw));
    if (!p.success || seen.has(p.data.componentName)) continue;
    seen.add(p.data.componentName);
    out.push({
      id: 100000 + idx,
      key: p.data.componentName,
      label: p.data.componentName,
      componentName: p.data.componentName,
      fields: p.data.fields.map((f) => f.name),
    });
    idx += 1;
  }
  const ok = z.array(ComponentDefSchema).safeParse(out);
  return ok.success ? ok.data : bundled;
}

async function fetchCompositeJSON(
  hash: string,
  opts: LoadSeedOptions,
): Promise<unknown | null> {
  const base = catalystBase(opts.base);
  const doFetch = opts.fetchImpl ?? fetch;
  try {
    const res = await doFetch(`${base}/content/contents/${hash}`, {
      signal: opts.signal,
      headers: { accept: "application/json" },
    });
    if (!res.ok) return null;
    const raw: unknown = await res.json();
    const parsed = CompositeSchema.safeParse(raw);
    if (!parsed.success) {
      console.warn("[catalyst] main.composite failed schema validation", parsed.error.issues);
      track(
        "catalyst_schema_drift",
        {
          module: "creator-hub/scene-editor",
          path: `/content/contents/${hash}`,
          issues: parsed.error.issues
            .slice(0, 5)
            .map((i) => `${i.path.map(String).join(".")}:${i.code}`),
        },
        { sid: "schema-drift" },
      );
      return raw;
    }
    return parsed.data;
  } catch {
    return null;
  }
}

export type ActiveEntity = {
  info: SceneInfo;
  content: { file: string; hash: string }[];
};

export async function fetchActiveEntity(
  opts: LoadSeedOptions = {},
): Promise<ActiveEntity | null> {
  const base = catalystBase(opts.base);
  const pointer = opts.pointer?.trim();
  if (!pointer) return null;
  const url = `${base}/content/entities/active`;
  const doFetch = opts.fetchImpl ?? fetch;

  try {
    const res = await doFetch(url, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({ pointers: [pointer] }),
      signal: opts.signal,
    });
    if (!res.ok) return null;
    const raw = (await res.json()) as unknown;
    if (!Array.isArray(raw) || raw.length === 0) return null;

    const parsed = ActiveEntitySchema.safeParse(raw[0]);
    if (!parsed.success) return null;
    const e = parsed.data;
    if (e.type !== "scene") return null;

    return {
      info: {
        pointer,
        title: e.metadata?.display?.title ?? "Untitled scene",
        base: e.metadata?.scene?.base ?? pointer,
        parcels: e.pointers,
        contentCount: e.content.length,
        live: true,
      },
      content: e.content,
    };
  } catch {
    return null;
  }
}

export function seedFromCompositeJSON(
  json: unknown,
  baseSeed: SceneEditorSeed,
  contentGlbs: string[] = [],
): SceneEditorSeed {
  try {
    const parsed = CompositeSchema.safeParse(json);
    if (!parsed.success) return baseSeed;

    const comps: CompMap = new Map();
    for (const c of parsed.data.components) comps.set(c.name, c.data);

    const hierarchy = deriveHierarchy(comps, baseSeed.scene.title);
    const glbPaths = dedupe([...placeholderSrcs(comps), ...contentGlbs]).slice(
      0,
      DERIVED_ASSET_CAP,
    );
    const assetCatalog = deriveAssetCatalog(baseSeed.assetCatalog, glbPaths);
    const components = deriveComponents(
      baseSeed.components,
      comps.get("inspector::Config") ?? {},
    );

    const hOk = z.array(HierarchyNodeSchema).safeParse(hierarchy);
    const aOk = AssetCatalogSchema.safeParse(assetCatalog);
    const cOk = z.array(ComponentDefSchema).safeParse(components);

    return {
      scene: baseSeed.scene,
      transformIdentity: baseSeed.transformIdentity,
      hierarchy: hOk.success && hOk.data.length > 0 ? hOk.data : baseSeed.hierarchy,
      assetCatalog: aOk.success ? aOk.data : baseSeed.assetCatalog,
      components: cOk.success && cOk.data.length > 0 ? cOk.data : baseSeed.components,
    };
  } catch {
    return baseSeed;
  }
}

export async function loadSceneEditorSeed(
  opts: LoadSeedOptions = {},
): Promise<SceneEditorSeed> {
  const seed = emptySeed(opts.pointer);

  const entity = await fetchActiveEntity(opts);
  if (!entity) return seed;
  seed.scene = entity.info;

  const composite = entity.content.find((c) => /(^|\/)main\.composite$/.test(c.file));
  if (!composite) return seed;

  try {
    const json = await fetchCompositeJSON(composite.hash, opts);
    if (json == null) return seed;
    const contentGlbs = entity.content.map((c) => c.file).filter((f) => GLB_RE.test(f));
    return seedFromCompositeJSON(json, seed, contentGlbs);
  } catch {
    return seed;
  }
}

export function buildViewportUrl(opts: {
  playUrl?: string;
  realm?: string | null;
  position?: string | null;
  preview?: boolean;
  systemScene?: string | null;
  editorUi?: boolean;
}): string {
  const playUrl = (opts.playUrl || "https://catalyst.example.com/play").replace(/\/+$/, "");
  const q = new URLSearchParams();
  if (opts.realm) q.set("realm", opts.realm);
  q.set("position", opts.position || "0,0");
  if (opts.preview) q.set("preview", "true");
  // Two independent flags. editorUi used to default to on whenever a system
  // scene was present, which read as "the system scene implies the editor
  // chrome" -- it does not: the overlay reads editorUi to decide whether to
  // mount the HUD, and the engine reads systemScene to decide what owns the UI.
  if (opts.systemScene) q.set("systemScene", opts.systemScene);
  if (opts.editorUi) q.set("editorUi", "1");
  return `${playUrl}/?${q.toString()}`;
}
