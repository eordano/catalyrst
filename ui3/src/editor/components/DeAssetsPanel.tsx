import { useMemo, useRef, useState } from "react";
import type { DeCatalogItem, DeLocalItem } from "../types";
import { PROJECT_CACHE, projectContentBase } from "../project-cache";
import { ModelGlyph } from "./DeIcons";
import { useOneShot } from "../use-one-shot";

/** One-shot request from the ribbon. The nonce is what lets the same request
    be made twice, after the user has navigated away in between. */
export interface DeAssetsPreset {
  nonce: number;
  tab?: "catalog" | "local";
  cat?: string;
  /** Restrict to smart items; combines with `cat` for one category shelf. */
  smart?: boolean;
  query?: string;
  focusSearch?: boolean;
}

export interface DeAssetsPanelProps {
  tab?: "catalog" | "local";
  preset?: DeAssetsPreset | null;
  width?: number;
  catalog?: DeCatalogItem[];
  local?: DeLocalItem[];
  live?: boolean;
  onPlace?: (asset: DeCatalogItem) => void;
}

export function DeAssetsPanel({
  tab = "catalog",
  preset = null,
  width = 300,
  catalog,
  local = [],
  live = false,
  onPlace = undefined,
}: DeAssetsPanelProps) {
  const [active, setActive] = useState(tab);
  useOneShot(preset?.nonce ?? 0, () => {
    if (preset?.tab) setActive(preset.tab);
  });
  return (
    <div className="eui-panel eui-left" style={{ width }}>
      <div className="eui-seg">
        {(["catalog", "local"] as const).map((t) => (
          <button
            key={t}
            className={"eui-seg-btn" + (active === t ? " active" : "")}
            onClick={() => setActive(t)}
          >
            {t === "catalog" ? "Catalog" : "Local"}
          </button>
        ))}
      </div>
      {active === "catalog" ? (
        <DeCatalogTab items={catalog} live={live} onPlace={onPlace} preset={preset} />
      ) : (
        <DeLocalTab items={local} live={live} onPlace={onPlace} />
      )}
    </div>
  );
}

function catOf(a: DeCatalogItem): string {
  return (a.category || a.pack || "").trim();
}

const CATALOG_RENDER_CAP = 240;
/** The select's token for the smart-only filter; never a real category name. */
const SMART_CATEGORY = "__smart";

export interface DeCatalogTabProps {
  items?: DeCatalogItem[];
  live?: boolean;
  onPlace?: (asset: DeCatalogItem) => void;
  preset?: DeAssetsPreset | null;
}

export function DeCatalogTab({
  items = [],
  live = false,
  onPlace = undefined,
  preset = null,
}: DeCatalogTabProps) {
  const placeable = typeof onPlace === "function";
  const [query, setQuery] = useState("");
  // Two orthogonal filter dimensions as two fields. The string protocol that
  // packed both into `cat` ("__smart:doors") produced values the select could
  // neither show nor emit.
  const [cat, setCat] = useState("");
  const [smartOnly, setSmartOnly] = useState(false);
  const searchRef = useRef<HTMLInputElement | null>(null);
  useOneShot(preset?.nonce ?? 0, () => {
    if (!preset) return;
    if (preset.query !== undefined) setQuery(preset.query);
    if (preset.cat !== undefined) setCat(preset.cat);
    if (preset.smart !== undefined) setSmartOnly(preset.smart);
    if (preset.focusSearch) searchRef.current?.focus();
  });

  // Case-insensitively deduped: the catalog ships "Seats" beside "doors" and
  // once shipped "Text" beside "text" -- one shelf per name, first spelling wins.
  const categories = useMemo(() => {
    const byLower = new Map<string, string>();
    for (const a of items) {
      const c = catOf(a);
      if (c && !byLower.has(c.toLowerCase())) byLower.set(c.toLowerCase(), c);
    }
    return Array.from(byLower.values()).sort((x, y) => x.localeCompare(y));
  }, [items]);
  const hasSmart = useMemo(() => items.some((a) => a.smart), [items]);

  const q = query.trim().toLowerCase();
  const catLower = cat.toLowerCase();
  const filtered = useMemo(() => {
    return items.filter((a) => {
      if (smartOnly && !a.smart) return false;
      if (catLower && catOf(a).toLowerCase() !== catLower) return false;
      if (!q) return true;
      const hay = `${a.name || ""} ${catOf(a)} ${a.pack || ""}${a.smart ? " smart item" : ""}`.toLowerCase();
      return hay.includes(q);
    });
  }, [items, catLower, smartOnly, q]);

  const shown = filtered.slice(0, CATALOG_RENDER_CAP);
  const truncated = filtered.length - shown.length;

  return (
    <>
      <div className="eui-search" style={{ display: "flex", gap: 6 }}>
        <input
          ref={searchRef}
          className="eui-input"
          style={{ flex: 1 }}
          placeholder={"Search models\u{2026}"}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          spellCheck={false}
        />
        <select
          className="eui-input"
          style={{ width: 120, flex: "none" }}
          value={smartOnly ? SMART_CATEGORY : cat}
          aria-label="Filter by category"
          onChange={(e) => {
            const v = e.target.value;
            setSmartOnly(v === SMART_CATEGORY);
            setCat(v === SMART_CATEGORY ? "" : v);
          }}
        >
          <option value="">All</option>
          {hasSmart && <option value={SMART_CATEGORY}>&#x26A1; Smart Items</option>}
          {categories.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </div>
      <div className="eui-asset-count">
        {q || cat
          ? `${filtered.length.toLocaleString()} of ${items.length.toLocaleString()} models`
          : `${items.length.toLocaleString()} models`}
      </div>
      {smartOnly && (
        <div
          className="eui-comp-note"
          style={{ padding: "0 12px 6px" }}
        >
          Smart items ship with built-in behaviors. Placing one adds its model &#x2014;
          wire what it does with &#x201C;Make interactive&#x201D; in the Inspector.
        </div>
      )}
      <div className="eui-panel-body" role="region" aria-label="Asset catalog" tabIndex={0}>
        <div className="eui-asset-grid">
          {filtered.length === 0 && (
            <div className="eui-empty" style={{ gridColumn: "1 / -1" }}>
              {items.length === 0
                ? "No models available"
                : `No models match${query.trim() ? ` \u{201C}${query.trim()}\u{201D}` : ""}`}
            </div>
          )}
          {shown.map((a) => (
            <div
              key={a.id}
              className={"eui-asset" + (live && !placeable ? " is-readonly" : "")}
              title={
                placeable
                  ? a.smart
                    ? `Place ${a.name} \u{2014} a smart item: wire what it does with \u{201C}Make interactive\u{201D} in the Inspector`
                    : `Place ${a.name} in the scene`
                  : `${a.name} \u{2014} ${catOf(a) || a.pack}`
              }
              onClick={placeable ? () => onPlace?.(a) : undefined}
            >
              <div
                className="thumb"
                style={
                  a.thumbnailUrl
                    ? undefined
                    : { background: `linear-gradient(150deg, hsl(${a.hue ?? 210} 60% 46%), hsl(${(a.hue ?? 210) + 30} 56% 28%))` }
                }
              >
                {a.thumbnailUrl ? (
                  <img
                    src={a.thumbnailUrl}
                    alt=""
                    loading="lazy"
                    style={{ width: "100%", height: "100%", objectFit: "contain" }}
                  />
                ) : (
                  <ModelGlyph />
                )}
              </div>
              <span className="name">{a.name}</span>
              <span className="pack">
                {a.smart ? "\u{26A1} " : ""}
                {catOf(a) || a.pack}
              </span>
            </div>
          ))}
          {truncated > 0 && (
            <div className="eui-asset-count" style={{ gridColumn: "1 / -1" }}>
              +{truncated.toLocaleString()} more &#x2014; refine your search to see them
            </div>
          )}
        </div>
      </div>
    </>
  );
}

const PROJECT_BASE_FALLBACK = "/_project/content";
const LOCAL_MODEL_RE = /\.(glb|gltf)$/i;

interface LocalModel {
  path: string;
  folder: string;
  url: string | null;
}

async function stashLocalModel(file: File): Promise<string | null> {
  if (typeof caches === "undefined" || typeof window === "undefined") return null;
  try {
    const base = (await projectContentBase()) ?? PROJECT_BASE_FALLBACK;
    const safe = file.name.replace(/[^\w.-]+/g, "_");
    const token = `local-${Date.now().toString(36)}-${safe}`;
    const url = new URL(`${base}/contents/${token}`, window.location.origin).href;
    const cache = await caches.open(PROJECT_CACHE);
    await cache.put(
      url,
      new Response(await file.arrayBuffer(), {
        headers: {
          "content-type": /\.gltf$/i.test(file.name) ? "model/gltf+json" : "model/gltf-binary",
          "access-control-allow-origin": "*",
        },
      }),
    );
    return url;
  } catch {
    return null;
  }
}

async function resolveProjectModelUrl(path: string): Promise<string | null> {
  if (typeof window === "undefined") return null;
  try {
    const base = (await projectContentBase()) ?? PROJECT_BASE_FALLBACK;
    const res = await fetch(new URL(`${base}/entities/active`, window.location.origin).href, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ pointers: ["0,0"] }),
    });
    if (!res.ok) return null;
    const arr = (await res.json()) as { content?: { file?: string; hash?: string }[] }[];
    const want = path.replace(/^\.\//, "").toLowerCase();
    for (const ent of Array.isArray(arr) ? arr : []) {
      for (const c of ent?.content ?? []) {
        if (
          typeof c?.file === "string" &&
          typeof c?.hash === "string" &&
          c.file.toLowerCase() === want
        ) {
          return new URL(`${base}/contents/${c.hash}`, window.location.origin).href;
        }
      }
    }
    return null;
  } catch {
    return null;
  }
}

function modelNameOf(path: string): string {
  return (path.split("/").pop() ?? path).replace(LOCAL_MODEL_RE, "");
}

export interface DeLocalTabProps {
  items?: DeLocalItem[];
  live?: boolean;
  onPlace?: (asset: DeCatalogItem) => void;
}

export function DeLocalTab({ items = [], live = false, onPlace = undefined }: DeLocalTabProps) {
  const placeable = typeof onPlace === "function";
  const [query, setQuery] = useState("");
  const [added, setAdded] = useState<LocalModel[]>([]);
  const [status, setStatus] = useState("");

  const all = useMemo<LocalModel[]>(
    () => [
      ...added,
      ...items.map((p) => ({ path: p.path, folder: p.folder || "model", url: null })),
    ],
    [items, added],
  );

  const q = query.trim().toLowerCase();
  const filtered = q ? all.filter((m) => m.path.toLowerCase().includes(q)) : all;

  const addFiles = async (list: FileList | null) => {
    const files = Array.from(list ?? []).filter((f) => LOCAL_MODEL_RE.test(f.name));
    if (files.length === 0) {
      setStatus("Only .glb / .gltf files can be added");
      return;
    }
    const next: LocalModel[] = [];
    for (const f of files) {
      const url = await stashLocalModel(f);
      if (url) next.push({ path: f.name, folder: "added this session", url });
    }
    setAdded((prev) => [...next, ...prev]);
    setStatus(
      next.length === files.length
        ? `Added ${next.length} model${next.length === 1 ? "" : "s"}${placeable ? " \u{2014} click it to place it in the scene" : ""}`
        : "Some files could not be stored \u{2014} try again",
    );
  };

  const refresh = async () => {
    if (typeof caches === "undefined") {
      setStatus("Up to date");
      return;
    }
    try {
      const cache = await caches.open(PROJECT_CACHE);
      const alive: LocalModel[] = [];
      for (const a of added) {
        if (a.url && (await cache.match(a.url))) alive.push(a);
      }
      setAdded(alive);
      setStatus(`Up to date \u{2014} ${(alive.length + items.length).toLocaleString()} models`);
    } catch {
      setStatus("Refresh failed \u{2014} try reloading the editor");
    }
  };

  const place = async (m: LocalModel) => {
    if (!placeable) return;
    const url = m.url ?? (await resolveProjectModelUrl(m.path));
    if (!url) {
      setStatus(`Couldn't find ${m.path} in the project files`);
      return;
    }
    const name = modelNameOf(m.path);
    onPlace?.({ id: `local:${m.path}`, name, glbUrl: url });
    setStatus(`Placed ${name}`);
  };

  return (
    <>
      <div className="eui-search" style={{ display: "flex", gap: 6 }}>
        <input
          className="eui-input"
          style={{ flex: 1 }}
          placeholder={"Filter local models\u{2026}"}
          value={query}
          spellCheck={false}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button
          className="eui-btn"
          title="Re-check this project's models"
          style={{ flex: "none" }}
          onClick={() => void refresh()}
        >
          &#x21BB;
        </button>
      </div>
      <div className="eui-asset-count">
        {q
          ? `${filtered.length.toLocaleString()} of ${all.length.toLocaleString()} models`
          : `${all.length.toLocaleString()} models in this project`}
      </div>
      {status !== "" && (
        <div
          className="eui-asset-count"
          role="status"
          style={{ textTransform: "none", letterSpacing: 0 }}
        >
          {status}
        </div>
      )}
      <div className="eui-panel-body" role="region" aria-label="Project models" tabIndex={0}>
        <div className="eui-asset-grid">
          <label className="eui-asset eui-asset-upload" title="Add a .glb / .gltf from your computer">
            <input
              type="file"
              accept=".glb,.gltf,model/gltf-binary,model/gltf+json"
              multiple
              style={{ display: "none" }}
              onChange={(e) => {
                void addFiles(e.target.files);
                e.target.value = "";
              }}
            />
            <div className="glyph">+</div>
            <span className="name">Add model</span>
            <span className="pack">from your computer</span>
          </label>
          {all.length === 0 && !q && (
            <div className="eui-empty" style={{ gridColumn: "1 / -1" }}>
              No models in this project yet &#x2014; add a .glb / .gltf to place it in the scene.
            </div>
          )}
          {all.length > 0 && filtered.length === 0 && (
            <div className="eui-empty" style={{ gridColumn: "1 / -1" }}>
              No local models match &#x201C;{query.trim()}&#x201D;
            </div>
          )}
          {filtered.map((m) => (
            <div
              key={m.url ?? m.path}
              className={"eui-asset" + (live && !placeable ? " is-readonly" : "")}
              title={placeable ? `Place ${m.path} in the scene` : m.path}
              onClick={placeable ? () => void place(m) : undefined}
            >
              <div className="glyph">
                <ModelGlyph />
              </div>
              <span className="name">{modelNameOf(m.path)}</span>
              <span className="pack">{m.folder}</span>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
