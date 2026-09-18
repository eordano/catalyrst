import type { LiveSceneInfo } from "../../generated/editor-bus";
import type { DeTreeNode } from "../types";

export interface DeSceneInfo {
  base?: string;
  parcels?: string[];
  template?: string | null;
}

interface DeSceneCardProps {
  title?: string;
  info?: DeSceneInfo | null;
  live?: LiveSceneInfo | null;
  metadata?: unknown;
  tree?: DeTreeNode[];
  onRename?: ((name: string) => void) | null;
}

interface SpawnPoint {
  name?: string;
  default?: boolean;
  position?: Record<string, unknown>;
}

interface SceneMeta {
  name: string;
  description: string;
  spawnPoints: SpawnPoint[];
}

function isObj(v: unknown): v is Record<string, unknown> {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

function unwrap(raw: unknown): Record<string, unknown> {
  if (!isObj(raw)) return {};
  const inner = raw.json ?? raw.value;
  if (typeof inner === "string") {
    try {
      const parsed: unknown = JSON.parse(inner);
      return isObj(parsed) ? parsed : {};
    } catch {
      return {};
    }
  }
  return isObj(inner) && !("name" in raw) ? inner : raw;
}

export function sceneMetaWithName(raw: unknown, name: string): Record<string, unknown> {
  return { ...unwrap(raw), name };
}

export function readSceneMeta(raw: unknown): SceneMeta {
  const m = unwrap(raw);
  const spawn = Array.isArray(m.spawnPoints)
    ? m.spawnPoints.filter(isObj).map((sp) => ({
        name: typeof sp.name === "string" ? sp.name : undefined,
        default: sp.default === true,
        position: isObj(sp.position) ? sp.position : undefined,
      }))
    : [];
  return {
    name: typeof m.name === "string" ? m.name : "",
    description: typeof m.description === "string" ? m.description : "",
    spawnPoints: spawn,
  };
}

function axis(v: unknown): string {
  if (typeof v === "number") return String(v);
  if (Array.isArray(v) && v.length >= 2) return `${String(v[0])}\u{2013}${String(v[1])}`;
  if (isObj(v) && typeof v.value === "number") return String(v.value);
  if (isObj(v) && Array.isArray(v.value)) return axis(v.value);
  return "?";
}

export function formatSpawn(sp: SpawnPoint): string {
  const p = sp.position ?? {};
  const at = `${axis(p.x)}, ${axis(p.y)}, ${axis(p.z)}`;
  const label = sp.name ? `${sp.name} at ${at}` : at;
  return sp.default ? `${label} (default)` : label;
}

export function countPlaced(nodes: DeTreeNode[] | undefined): number {
  let n = 0;
  for (const node of nodes ?? []) {
    if (String(node.id) !== "0") n += 1;
    n += countPlaced(node.children);
  }
  return n;
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="eui-prop">
      <span className="plabel">{label}</span>
      <span className="pvalue">{children}</span>
    </div>
  );
}

export default function DeSceneCard({
  title = "",
  info = null,
  live = null,
  metadata = undefined,
  tree = [],
  onRename = null,
}: DeSceneCardProps) {
  const meta = readSceneMeta(metadata);
  const name = meta.name || live?.title || title || "Untitled Scene";
  const parcels =
    live?.parcels && live.parcels.length > 0
      ? live.parcels.map((p) => `${p.x},${p.y}`)
      : (info?.parcels ?? []);
  const base = info?.base ?? parcels[0] ?? "";
  const placed = countPlaced(tree);
  return (
    <div className="eui-panel eui-right">
      <div className="eui-panel-head">
        <div className="eui-head-text">
          <span className="eui-overline">Scene</span>
          {onRename ? (
            <input
              key={name}
              className="eui-name-input"
              defaultValue={name}
              spellCheck={false}
              aria-label="Scene name"
              title="Rename the scene"
              onKeyDown={(e) => {
                if (e.key === "Escape") e.currentTarget.value = name;
                if (e.key === "Enter" || e.key === "Escape") e.currentTarget.blur();
              }}
              onBlur={(e) => {
                const next = e.currentTarget.value.trim();
                if (next && next !== name) onRename(next);
                else e.currentTarget.value = name;
              }}
            />
          ) : (
            <span className="eui-name-text" title="Scene name">
              {name}
            </span>
          )}
        </div>
        <span className="eui-id-badge">#0</span>
      </div>
      <div className="eui-panel-body" role="region" aria-label="Scene properties" tabIndex={0}>
        <Row label="Name">{name}</Row>
        {meta.description ? <Row label="Description">{meta.description}</Row> : null}
        <Row label="Parcels">
          {parcels.length === 0
            ? "Not set"
            : `${parcels.length} ${parcels.length === 1 ? "parcel" : "parcels"}: ${parcels.join("  ")}`}
        </Row>
        <Row label="Base parcel">{base || "Not set"}</Row>
        <Row label="Spawn point">
          {meta.spawnPoints.length === 0
            ? "Not set \u{2014} the engine picks a default"
            : meta.spawnPoints.map(formatSpawn).join("; ")}
        </Row>
        <Row label="Items">{placed === 0 ? "None placed yet" : String(placed)}</Row>
        {info?.template ? <Row label="Template">{info.template}</Row> : null}
        {live?.sdkVersion ? <Row label="SDK">{live.sdkVersion}</Row> : null}
        <div className="eui-comp-note">
          This is the scene itself. Select an item in the hierarchy to edit its components, or add
          one from Insert.
        </div>
      </div>
    </div>
  );
}
