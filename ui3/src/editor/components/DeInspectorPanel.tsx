import type { ReactNode } from "react";
import { useId, useMemo, useState } from "react";
import type { AuthorComponentFn, DeleteComponentFn, EditorTransform } from "../types";
import Modal from "../../components/Modal";
import DeInteractionsPanel, { type DeInteractionsPreset } from "./DeInteractionsPanel";
import { IconBolt, IconPlus, IconTrash } from "./DeIcons";
import { useOneShot } from "../use-one-shot";
import {
  AxisRow,
  BoolField,
  NumField,
  PropRow,
  TextField,
  atPath,
  hexToRgb,
  rgbToHex,
  withPath,
  type CompValue,
  type NudgeFieldFn,
  type WriteComp,
} from "./DeInspectorFields";
import {
  ACTION_TYPE_OPTIONS,
  ActionEntry,
  EntryListShell,
  TriggerEntry,
  entryList,
} from "./DeInspectorSmartItems";

export type { NudgeFieldFn } from "./DeInspectorFields";

interface ComponentJsonModalProps {
  name: string;
  value?: unknown;
  onClose: () => void;
  onSave: (text: string) => void;
}

function ComponentJsonModal({ name, value, onClose, onSave }: ComponentJsonModalProps) {
  const initial = useMemo(() => {
    try {
      return JSON.stringify(value ?? {}, null, 2);
    } catch {
      return "{}";
    }
  }, [value]);
  const [text, setText] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const save = () => {
    try {
      JSON.parse(text);
    } catch (e) {
      setError("Invalid JSON \u{2014} " + (e instanceof Error ? e.message : "parse error"));
      return;
    }
    setError(null);
    onSave(text);
  };
  return (
    <Modal onClose={onClose} width={520} ariaLabel={`Edit ${name} as JSON`}>
      <div className="eui-json-modal">
        <div className="eui-json-modal-head" style={{ fontWeight: 600, marginBottom: 8 }}>
          Edit {name} as JSON
        </div>
        <textarea
          className="eui-input"
          spellCheck={false}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            if (error) setError(null);
          }}
          rows={14}
          style={{ width: "100%", fontFamily: "monospace", minHeight: 220, resize: "vertical" }}
          autoFocus
        />
        {error && (
          <p role="alert" style={{ color: "var(--error, #e5484d)", fontSize: 12, marginTop: 6 }}>{error}</p>
        )}
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 10 }}>
          <button className="eui-btn" onClick={onClose}>Cancel</button>
          <button className="eui-btn primary" onClick={save}>Save</button>
        </div>
      </div>
    </Modal>
  );
}

interface CompCardProps {
  ns?: string | null;
  name: string;
  rawName?: string | null;
  entityId?: string | number | null;
  value?: unknown;
  expanded?: boolean;
  readonly?: boolean;
  hasJson?: boolean;
  live?: boolean;
  onAuthorComponent?: AuthorComponentFn;
  onDelete?: () => void;
  children?: ReactNode;
}

function CompCard({
  ns = null,
  name,
  rawName = null,
  entityId = null,
  value = undefined,
  expanded = true,
  readonly = false,
  hasJson = true,
  onAuthorComponent = undefined,
  onDelete = undefined,
  children,
}: CompCardProps) {
  const [open, setOpen] = useState(expanded);
  const [jsonOpen, setJsonOpen] = useState(false);
  const canEditJson =
    typeof onAuthorComponent === "function" && rawName != null && entityId != null;
  return (
    <div className="eui-comp">
      <div
        className={"eui-comp-head" + (readonly ? " readonly" : "")}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="twisty">{open ? "\u{25BE}" : "\u{25B8}"}</span>
        <span className="name">
          {ns && <span className="ns">{ns} / </span>}
          {name}
        </span>
        <span className="spacer" />
        {open && !readonly && hasJson && (
          <button
            className="eui-link"
            title={canEditJson ? "Edit this component as JSON" : "Edit as JSON"}
            aria-label="Edit as JSON"
            disabled={!canEditJson}
            onClick={(e) => {
              e.stopPropagation();
              setJsonOpen(true);
            }}
          >
            json
          </button>
        )}
        <button
          className="eui-btn icon"
          style={{ width: 20, height: 20 }}
          title="Remove component"
          aria-label="Remove component"
          disabled={!onDelete}
          onClick={
            onDelete
              ? (e) => {
                  e.stopPropagation();
                  onDelete();
                }
              : undefined
          }
        >
          <IconTrash />
        </button>
      </div>
      {open && <div className="eui-comp-body">{children}</div>}
      {jsonOpen && canEditJson && (
        <ComponentJsonModal
          name={name}
          value={value}
          onClose={() => setJsonOpen(false)}
          onSave={(text) => {
            onAuthorComponent?.(entityId, rawName as string, text);
            setJsonOpen(false);
          }}
        />
      )}
    </div>
  );
}

const HIDDEN_COMPONENTS = new Set<string>([
  "composite::root",
  "core-schema::Name",
  "core-schema::Network-Entity",
  "core-schema::Sync-Components",
  "core-schema::Tags",
  "inspector::Selection",
  "inspector::Nodes",
  "inspector::TransformConfig",
  "inspector::SceneMetadata-v3",
  "inspector::Config",
  "asset-packs::Placeholder",
]);

export const DUPLICATE_SKIP = new Set<string>([...HIDDEN_COMPONENTS, "Name"]);

export const isTransformComp = (name: string) => name === "Transform" || name === "core::Transform";

const NS_LABEL: Record<string, string | null> = {
  core: null,
  "core-schema": null,
  "asset-packs": "Smart Item",
  inspector: "Inspector",
};

function splitComp(name: string): { nsLabel: string | null; label: string } {
  const i = name.indexOf("::");
  const ns = i === -1 ? null : name.slice(0, i);
  const raw = i === -1 ? name : name.slice(i + 2);
  const label =
    raw
      .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
      .replace(/[-_]+/g, " ")
      .trim() || name;
  const nsLabel = ns == null ? null : ns in NS_LABEL ? NS_LABEL[ns] ?? null : ns;
  return { nsLabel, label };
}

const CANONICAL_COMPONENT: Record<string, string> = {
  GltfContainer: "core::GltfContainer",
  GltfContainerLoadingState: "core::GltfContainerLoadingState",
  Material: "core::Material",
  MeshRenderer: "core::MeshRenderer",
  MeshCollider: "core::MeshCollider",
  VisibilityComponent: "core::VisibilityComponent",
  VideoPlayer: "core::VideoPlayer",
  Actions: "asset-packs::Actions",
  Triggers: "asset-packs::Triggers",
};

const GLTF_LOADING_STATE: Record<number, string> = {
  0: "Unknown",
  1: "Loading\u{2026}",
  2: "Not found \u{2014} the model file is missing",
  3: "Failed to load",
  4: "Loaded",
};

function bodyFor(
  name: string,
  transform: EditorTransform | null | undefined,
  live = false,
  uid = "",
  onNudge?: NudgeFieldFn,
  value?: CompValue,
  onWrite?: WriteComp,
  sibling: Record<string, unknown> = {},
): ReactNode {
  const v: CompValue = value ?? {};
  const num = (path: string[], fallback: number): number => {
    const n = atPath(v, path);
    return typeof n === "number" ? n : fallback;
  };
  const str = (path: string[], fallback = ""): string => {
    const t = atPath(v, path);
    return typeof t === "string" ? t : fallback;
  };
  const bool = (path: string[], fallback: boolean): boolean => {
    const b = atPath(v, path);
    return typeof b === "boolean" ? b : fallback;
  };
  const set = (path: string[]) =>
    onWrite ? (leaf: unknown) => onWrite(withPath(v, path, leaf)) : undefined;
  const meshShapeSelect = (id: string, keep: CompValue) => (
    <select
      id={id}
      className="eui-select"
      value={str(["mesh", "$case"], "box")}
      disabled={onWrite === undefined}
      onChange={(e) => onWrite?.({ ...keep, mesh: { $case: e.target.value, [e.target.value]: {} } })}
    >
      {["box", "sphere", "cylinder", "plane"].map((m) => (
        <option key={m} value={m}>
          {m}
        </option>
      ))}
    </select>
  );
  switch (CANONICAL_COMPONENT[name] ?? name) {
    case "Transform":
    case "core::Transform":
      return (
        <>
          <AxisRow
            label="position"
            v={transform?.position ?? { x: 0, y: 0, z: 0 }}
            readOnly={live}
            onNudge={onNudge ? (ax, n) => onNudge("position", ax, n) : undefined}
          />
          <AxisRow
            label={"rotation \u{B0}"}
            v={transform?.rotation ?? { x: 0, y: 0, z: 0 }}
            readOnly={live}
            onNudge={onNudge ? (ax, n) => onNudge("rotation", ax, n) : undefined}
          />
          <AxisRow
            label="scale"
            v={transform?.scale ?? { x: 1, y: 1, z: 1 }}
            readOnly={live}
            onNudge={onNudge ? (ax, n) => onNudge("scale", ax, n) : undefined}
          />
        </>
      );
    case "core::Material": {
      const pbr = ["material", "pbr"];
      return (
        <>
          <div className="eui-group-label">pbr</div>
          <PropRow label="albedo color" htmlFor={uid + "-albedo"}>
            <input
              id={uid + "-albedo"}
              type="color"
              className="eui-color-swatch"
              value={rgbToHex(atPath(v, [...pbr, "albedoColor"]))}
              disabled={onWrite === undefined}
              onChange={(e) => set([...pbr, "albedoColor"])?.(hexToRgb(e.target.value, num([...pbr, "albedoColor", "a"], 1)))}
            />
            <span className="eui-axis">
              <span className="ax">A</span>
              <NumField
                id={uid + "-albedo-a"}
                value={num([...pbr, "albedoColor", "a"], 1)}
                onCommit={set([...pbr, "albedoColor", "a"])}
              />
            </span>
          </PropRow>
          <PropRow label="metallic" htmlFor={uid + "-metallic"}>
            <span className="eui-axis">
              <span className="ax">N</span>
              <NumField id={uid + "-metallic"} value={num([...pbr, "metallic"], 0.5)} onCommit={set([...pbr, "metallic"])} />
            </span>
          </PropRow>
          <PropRow label="roughness" htmlFor={uid + "-roughness"}>
            <span className="eui-axis">
              <span className="ax">N</span>
              <NumField id={uid + "-roughness"} value={num([...pbr, "roughness"], 0.5)} onCommit={set([...pbr, "roughness"])} />
            </span>
          </PropRow>
          <PropRow label="cast shadows">
            <BoolField checked={bool([...pbr, "castShadows"], true)} label="cast shadows" onCommit={set([...pbr, "castShadows"])} />
          </PropRow>
          <PropRow label="texture" htmlFor={uid + "-tex"}>
            <TextField
              id={uid + "-tex"}
              value={str([...pbr, "texture", "tex", "texture", "src"])}
              placeholder="texture.png"
              onCommit={set([...pbr, "texture", "tex", "texture", "src"])}
            />
          </PropRow>
        </>
      );
    }
    case "core::MeshRenderer":
      return (
        <>
          <div className="eui-group-label">mesh</div>
          <PropRow label="primitive" htmlFor={uid + "-primitive"}>
            {meshShapeSelect(uid + "-primitive", {})}
          </PropRow>
        </>
      );
    case "core::MeshCollider":
      return (
        <PropRow label="collider" htmlFor={uid + "-collider"}>
          {meshShapeSelect(uid + "-collider", v)}
        </PropRow>
      );
    case "core::VisibilityComponent":
      return (
        <PropRow label="visible">
          <BoolField checked={bool(["visible"], true)} label="visible" onCommit={set(["visible"])} />
        </PropRow>
      );
    case "core::VideoPlayer":
      return (
        <>
          <PropRow label="src" htmlFor={uid + "-video-src"}>
            <TextField
              id={uid + "-video-src"}
              value={str(["src"])}
              placeholder="video url or file"
              onCommit={set(["src"])}
            />
          </PropRow>
          <PropRow label="playing">
            <BoolField checked={bool(["playing"], true)} label="playing" onCommit={set(["playing"])} />
          </PropRow>
          <PropRow label="volume" htmlFor={uid + "-video-volume"}>
            <span className="eui-axis">
              <span className="ax">N</span>
              <NumField id={uid + "-video-volume"} value={num(["volume"], 1)} onCommit={set(["volume"])} />
            </span>
          </PropRow>
        </>
      );
    case "core::GltfContainer":
      return (
        <PropRow label="src" htmlFor={uid + "-gltf-src"}>
          <TextField
            id={uid + "-gltf-src"}
            value={str(["src"])}
            placeholder="model.glb"
            onCommit={set(["src"])}
          />
        </PropRow>
      );
    case "core::GltfContainerLoadingState":
      return (
        <PropRow label="Model">
          {GLTF_LOADING_STATE[num(["currentState"], 0)] ?? "Unknown"}
        </PropRow>
      );
    case "asset-packs::Actions": {
      const entries = entryList(v);
      const rd = onWrite === undefined;
      const patchValue = (list: Record<string, unknown>[]) =>
        onWrite?.({ ...v, value: list });
      return (
        <>
          <EntryListShell
            title="actions"
            emptyHint="No actions yet -- add one, or use Make interactive above."
            count={entries.length}
            addLabel="add action"
            disabled={rd}
            onAdd={() =>
              patchValue([...entries, { name: "New action", type: "", jsonPayload: "{}" }])
            }
          >
            {entries.map((e, i) => (
              <ActionEntry
                key={i}
                entry={e}
                uid={uid + "-a" + i}
                readonly={rd}
                onPatch={(next) => patchValue(entries.map((x, j) => (j === i ? next : x)))}
                onRemove={() => patchValue(entries.filter((_, j) => j !== i))}
              />
            ))}
          </EntryListShell>
          <datalist id={uid + "-names"}>
            {entries.map((e, i) => {
              const n = typeof e.name === "string" ? e.name : "";
              return n ? <option key={i} value={n} /> : null;
            })}
          </datalist>
          <datalist id={uid + "-types"}>
            {ACTION_TYPE_OPTIONS.map((t) => (
              <option key={t} value={t} />
            ))}
          </datalist>
        </>
      );
    }
    case "asset-packs::Triggers": {
      const entries = entryList(v);
      const rd = onWrite === undefined;
      const patchValue = (list: Record<string, unknown>[]) =>
        onWrite?.({ ...v, value: list });
      const actionsRaw = sibling?.["asset-packs::Actions"];
      const actionNames: string[] = [];
      if (actionsRaw !== null && typeof actionsRaw === "object") {
        for (const a of entryList(actionsRaw as CompValue)) {
          const n = a["name"];
          if (typeof n === "string" && n) actionNames.push(n);
        }
      }
      return (
        <>
          <EntryListShell
            title="triggers"
            emptyHint="No triggers yet -- add one, or use Make interactive above."
            count={entries.length}
            addLabel="add trigger"
            disabled={rd}
            onAdd={() =>
              patchValue([...entries, { type: "on_input_action", actions: [{ name: "" }] }])
            }
          >
            {entries.map((e, i) => (
              <TriggerEntry
                key={i}
                entry={e}
                uid={uid + "-t" + i}
                readonly={rd}
                onPatch={(next) => patchValue(entries.map((x, j) => (j === i ? next : x)))}
                onRemove={() => patchValue(entries.filter((_, j) => j !== i))}
              />
            ))}
          </EntryListShell>
          <datalist id={uid + "-names"}>
            {actionNames.map((n) => (
              <option key={n} value={n} />
            ))}
          </datalist>
        </>
      );
    }
    default:
      return null;
  }
}

interface RealComponentCardsProps {
  componentValues?: Record<string, unknown>;
  components?: string[] | null;
  transform?: EditorTransform | null;
  live?: boolean;
  entityId?: string | number | null;
  onAuthorComponent?: AuthorComponentFn;
  onDeleteComponent?: DeleteComponentFn;
  onNudgeTransform?: NudgeFieldFn;
}

function RealComponentCards({
  componentValues,
  components,
  transform,
  live = false,
  entityId = null,
  onAuthorComponent = undefined,
  onDeleteComponent = undefined,
  onNudgeTransform = undefined,
}: RealComponentCardsProps) {
  const uid = useId();
  const isTransformName = (c: string) => c === "core::Transform" || c === "Transform";
  const visible = (components ?? []).filter((c) => !HIDDEN_COMPONENTS.has(c));
  if (visible.length === 0) {
    return <div className="eui-empty">No editable components on this entity &#x2014; add one with +</div>;
  }
  const xf = visible.find(isTransformName);
  const ordered = xf ? [xf, ...visible.filter((c) => !isTransformName(c))] : visible;
  return (
    <>
      {ordered.map((cname) => {
        const { nsLabel, label } = splitComp(cname);
        const isTransform = isTransformName(cname);
        const raw = componentValues?.[cname];
        const compValue =
          raw !== null && typeof raw === "object" ? (raw as Record<string, unknown>) : undefined;
        const body = bodyFor(
          cname,
          transform,
          live,
          uid + cname.replace(/[^a-zA-Z0-9]+/g, "-"),
          isTransform ? onNudgeTransform : undefined,
          compValue,
          onAuthorComponent && entityId != null
            ? (next) => onAuthorComponent(entityId, cname, JSON.stringify(next))
            : undefined,
          componentValues ?? {},
        );
        return (
          <CompCard
            key={cname}
            ns={nsLabel}
            name={label}
            rawName={cname}
            entityId={entityId}
            value={isTransform ? transform : undefined}
            expanded={isTransform || body !== null}
            hasJson={!isTransform}
            live={live}
            onAuthorComponent={onAuthorComponent}
            onDelete={
              onDeleteComponent && entityId != null
                ? () => onDeleteComponent(entityId, cname)
                : undefined
            }
          >
            {body ? (
              isTransform ? (
                live ? (
                  <>
                    {body}
                    <div className="eui-comp-note">
                      Drag the gizmo on the canvas, or focus a field and press &#x2191;/&#x2193; to nudge
                      (Shift for &#xB1;0.01).
                    </div>
                  </>
                ) : (
                  body
                )
              ) : (
                <>
                  {body}
                  {componentValues?.[cname] === undefined ? (
                    <div className="eui-comp-note">
                      No value from the scene yet &#x2014; showing defaults.
                    </div>
                  ) : null}
                </>
              )
            ) : (
              <div className="eui-comp-note">No inline fields &#x2014; edit this component as JSON.</div>
            )}
          </CompCard>
        );
      })}
    </>
  );
}

interface DeInspectorPanelProps {
  componentValues?: Record<string, unknown>;
  name?: string;
  id?: string | number;
  addOpen?: boolean;
  components?: string[] | null;
  transform?: EditorTransform | null;
  live?: boolean;
  onAuthorComponent?: AuthorComponentFn;
  onDeleteComponent?: DeleteComponentFn;
  onNudgeTransform?: NudgeFieldFn;
  interactionsOpen?: boolean;
  revealNonce?: number;
  interactionsPreset?: DeInteractionsPreset | null;
}

export function DeInspectorPanel({
  name = "",
  id = "",
  addOpen = false,
  componentValues = undefined,
  components = null,
  transform = null,
  live = false,
  onAuthorComponent = undefined,
  onDeleteComponent = undefined,
  onNudgeTransform = undefined,
  interactionsOpen = false,
  revealNonce = 0,
  interactionsPreset = null,
}: DeInspectorPanelProps) {
  const [interOpen, setInterOpen] = useState(interactionsOpen);
  const [localAddOpen, setLocalAddOpen] = useState(addOpen);
  const addPickerOpen = localAddOpen;
  useOneShot(revealNonce, () => {
    if (interactionsOpen) setInterOpen(true);
    if (addOpen) setLocalAddOpen(true);
  });
  return (
    <div className="eui-panel eui-right">
      <div className="eui-panel-head">
        <div className="eui-head-text">
          <span className="eui-overline">Inspector</span>
          <input
            key={name}
            className="eui-name-input"
            defaultValue={name}
            spellCheck={false}
            aria-label="Entity name"
            title="Entity name"
            readOnly
          />
        </div>
        {id !== "" && id != null ? <span className="eui-id-badge">#{id}</span> : null}
        <button
          className={"eui-btn" + (interOpen ? " active" : "")}
          style={{ padding: "0 8px", fontSize: 12, flex: "none" }}
          title={"Make this item interactive \u{2014} pick a trigger (click, press E) and what happens. No code."}
          aria-label="Add interaction"
          aria-pressed={interOpen}
          onClick={() => setInterOpen((v) => !v)}
        >
          <IconBolt />
          Make interactive
        </button>
        <button
          className={"eui-btn icon" + (addPickerOpen ? " active" : "")}
          title="Add component"
          aria-label="Add component"
          aria-expanded={addPickerOpen}
          disabled={!onAuthorComponent}
          onClick={() => setLocalAddOpen((v) => !v)}
        >
          <IconPlus />
        </button>
      </div>
      <div className="eui-panel-body" role="region" aria-label="Entity components" tabIndex={0}>
        {addPickerOpen && (
          <DeAddComponentPicker
            onPick={
              onAuthorComponent
                ? (compName) => {
                    onAuthorComponent(id, compName, "{}");
                    setLocalAddOpen(false);
                  }
                : undefined
            }
          />
        )}
        {interOpen && (
          <DeInteractionsPanel
            entityId={id}
            entityName={name}
            onWrite={
              onAuthorComponent ? (cname, json) => onAuthorComponent(id, cname, json) : null
            }
            preset={interactionsPreset}
          />
        )}

        <RealComponentCards
          componentValues={componentValues}
          components={components ?? []}
          transform={transform}
          live={live}
          entityId={id}
          onAuthorComponent={onAuthorComponent}
          onDeleteComponent={onDeleteComponent}
          onNudgeTransform={onNudgeTransform}
        />
      </div>
    </div>
  );
}

type AddComponentGroup = "3D Content" | "Interaction";

interface AddComponentDef {
  name: string;
  label: string;
  group: AddComponentGroup;
}

const ADD_COMPONENTS: readonly AddComponentDef[] = [
  { name: "GltfContainer", label: "3D model", group: "3D Content" },
  { name: "VisibilityComponent", label: "Show / hide", group: "3D Content" },
  { name: "Animator", label: "Animation", group: "3D Content" },
  { name: "Billboard", label: "Always face the player", group: "3D Content" },
  { name: "NftShape", label: "NFT picture frame", group: "3D Content" },
  { name: "PointerEvents", label: "Clickable", group: "Interaction" },
  { name: "AudioSource", label: "Sound", group: "Interaction" },
  { name: "TextShape", label: "Text label", group: "Interaction" },
];

const ADD_GROUP_ORDER: readonly AddComponentGroup[] = ["3D Content", "Interaction"];

function DeAddComponentPicker({ onPick = undefined }: { onPick?: (name: string) => void }) {
  return (
    <div className="eui-pop">
      <div className="eui-pop-list">
        {ADD_GROUP_ORDER.map((group) => {
          const items = ADD_COMPONENTS.filter((c) => c.group === group);
          return (
            <div key={group}>
              <div className="eui-group-label">{group}</div>
              {items.map((c) => (
                <div
                  key={c.name}
                  className="eui-pop-item"
                  role={onPick ? "button" : undefined}
                  tabIndex={onPick ? 0 : undefined}
                  title={c.name}
                  onClick={onPick ? () => onPick(c.name) : undefined}
                  onKeyDown={
                    onPick
                      ? (e) => {
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            onPick(c.name);
                          }
                        }
                      : undefined
                  }
                >
                  {c.label}
                  <span className="hint">{c.name}</span>
                </div>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
