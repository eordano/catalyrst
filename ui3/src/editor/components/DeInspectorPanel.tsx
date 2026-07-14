import type { ReactNode } from "react";
import { useId, useMemo, useState } from "react";
import type {
  AuthorComponentFn,
  DeleteComponentFn,
  EditorTransform,
  EditorVec,
} from "../types";
import { nudgeFromKey } from "../transform-nudge";
import Modal from "../../components/Modal";
import DeInteractionsPanel from "./DeInteractionsPanel";
import { IconBolt, IconPlus, IconTrash } from "./DeIcons";

type NudgeAxisFn = (axis: keyof EditorVec, delta: number) => void;

interface AxisRowProps {
  label: string;
  v: EditorVec;
  axes?: readonly (keyof EditorVec)[];
  readOnly?: boolean;
  onNudge?: NudgeAxisFn;
}

function AxisRow({ label, v, axes = ["x", "y", "z"], readOnly = false, onNudge }: AxisRowProps) {
  return (
    <div className="eui-prop">
      <span className="plabel">{label}</span>
      <span className="pvalue">
        {axes.map((ax) => (
          <span className="eui-axis" key={ax}>
            <span
              className="ax"
              title={onNudge ? "↑/↓ nudge ±1 · shift ±0.01" : "drag to scrub · shift for fine"}
            >
              {ax.toUpperCase()}
            </span>
            <input
              className="eui-num"
              aria-label={`${label} ${ax.toUpperCase()}`}
              {...(onNudge ? { value: v[ax] } : { defaultValue: v[ax] })}
              readOnly={readOnly}
              spellCheck={false}
              onKeyDown={
                onNudge
                  ? (e) => {
                      const delta = nudgeFromKey(0, e.key, e.shiftKey);
                      if (delta !== null) {
                        e.preventDefault();
                        onNudge(ax, delta);
                      }
                    }
                  : undefined
              }
            />
          </span>
        ))}
      </span>
    </div>
  );
}

interface PropRowProps {
  label: string;
  htmlFor?: string;
  children?: ReactNode;
}

function PropRow({ label, htmlFor, children }: PropRowProps) {
  return (
    <div className="eui-prop">
      {htmlFor ? (
        <label className="plabel" htmlFor={htmlFor}>{label}</label>
      ) : (
        <span className="plabel">{label}</span>
      )}
      <span className="pvalue">{children}</span>
    </div>
  );
}

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
      setError("Invalid JSON — " + (e instanceof Error ? e.message : "parse error"));
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
        <span className="twisty">{open ? "▾" : "▸"}</span>
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

export type NudgeFieldFn = (
  field: "position" | "rotation" | "scale",
  axis: keyof EditorVec,
  delta: number,
) => void;

function bodyFor(
  name: string,
  transform: EditorTransform | null | undefined,
  live = false,
  uid = "",
  onNudge?: NudgeFieldFn,
): ReactNode {
  switch (name) {
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
            label="rotation °"
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
    case "core::Material":
      return (
        <>
          <div className="eui-group-label">pbr</div>
          <PropRow label="albedo color" htmlFor={uid + "-albedo-a"}>
            <span className="eui-color-swatch" style={{ background: "#ffffff" }} />
            <span className="eui-axis"><span className="ax">A</span><input id={uid + "-albedo-a"} className="eui-num" defaultValue={1} /></span>
          </PropRow>
          <PropRow label="metallic" htmlFor={uid + "-metallic"}>
            <span className="eui-axis"><span className="ax">N</span><input id={uid + "-metallic"} className="eui-num" defaultValue={0} /></span>
          </PropRow>
          <PropRow label="roughness" htmlFor={uid + "-roughness"}>
            <span className="eui-axis"><span className="ax">N</span><input id={uid + "-roughness"} className="eui-num" defaultValue={0.5} /></span>
          </PropRow>
          <PropRow label="cast shadows">
            <span className="eui-toggle on" />
          </PropRow>
        </>
      );
    case "core::MeshRenderer":
      return (
        <>
          <div className="eui-group-label">mesh</div>
          <PropRow label="primitive" htmlFor={uid + "-primitive"}>
            <select id={uid + "-primitive"} className="eui-select" defaultValue="box">
              <option value="box">box</option>
              <option value="sphere">sphere</option>
              <option value="cylinder">cylinder</option>
              <option value="plane">plane</option>
            </select>
          </PropRow>
        </>
      );
    case "core::MeshCollider":
      return (
        <PropRow label="collider" htmlFor={uid + "-collider"}>
          <select id={uid + "-collider"} className="eui-select" defaultValue="box">
            <option value="box">box</option>
            <option value="plane">plane</option>
            <option value="sphere">sphere</option>
          </select>
        </PropRow>
      );
    case "core::VisibilityComponent":
      return (
        <PropRow label="visible">
          <span className="eui-toggle on" />
        </PropRow>
      );
    case "core::VideoPlayer":
      return (
        <>
          <PropRow label="src" htmlFor={uid + "-video-src"}>
            <input id={uid + "-video-src"} className="eui-input" defaultValue="" placeholder="video url or file" spellCheck={false} />
          </PropRow>
          <PropRow label="playing">
            <span className="eui-toggle on" />
          </PropRow>
          <PropRow label="volume" htmlFor={uid + "-video-volume"}>
            <span className="eui-axis"><span className="ax">N</span><input id={uid + "-video-volume"} className="eui-num" defaultValue={1} /></span>
          </PropRow>
        </>
      );
    case "core::GltfContainer":
      return (
        <PropRow label="src" htmlFor={uid + "-gltf-src"}>
          <input id={uid + "-gltf-src"} className="eui-input" defaultValue="" placeholder="model.glb" spellCheck={false} />
        </PropRow>
      );
    default:
      return null;
  }
}

interface RealComponentCardsProps {
  components?: string[] | null;
  transform?: EditorTransform | null;
  live?: boolean;
  entityId?: string | number | null;
  onAuthorComponent?: AuthorComponentFn;
  onDeleteComponent?: DeleteComponentFn;
  onNudgeTransform?: NudgeFieldFn;
}

function RealComponentCards({
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
    return <div className="eui-empty">No editable components on this entity — add one with +</div>;
  }
  const xf = visible.find(isTransformName);
  const ordered = xf ? [xf, ...visible.filter((c) => !isTransformName(c))] : visible;
  return (
    <>
      {ordered.map((cname) => {
        const { nsLabel, label } = splitComp(cname);
        const isTransform = isTransformName(cname);
        const body = bodyFor(
          cname,
          transform,
          live,
          uid + cname.replace(/[^a-zA-Z0-9]+/g, "-"),
          isTransform ? onNudgeTransform : undefined,
        );
        return (
          <CompCard
            key={cname}
            ns={nsLabel}
            name={label}
            rawName={cname}
            entityId={entityId}
            value={isTransform ? transform : undefined}
            expanded={isTransform}
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
                      Drag the gizmo on the canvas, or focus a field and press ↑/↓ to nudge
                      (Shift for ±0.01).
                    </div>
                  </>
                ) : (
                  body
                )
              ) : (
                <>
                  {body}
                  <div className="eui-comp-note">
                    Sample values — representative defaults, not read from the scene.
                  </div>
                </>
              )
            ) : (
              <div className="eui-comp-note">No inline fields — edit this component as JSON.</div>
            )}
          </CompCard>
        );
      })}
    </>
  );
}

export interface DeInspectorPanelProps {
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
}

export function DeInspectorPanel({
  name = "",
  id = "",
  addOpen = false,
  components = null,
  transform = null,
  live = false,
  onAuthorComponent = undefined,
  onDeleteComponent = undefined,
  onNudgeTransform = undefined,
  interactionsOpen = false,
}: DeInspectorPanelProps) {
  const [interOpen, setInterOpen] = useState(interactionsOpen);
  const [localAddOpen, setLocalAddOpen] = useState(addOpen);
  const addPickerOpen = addOpen || localAddOpen;
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
            readOnly={live}
            aria-label="Entity name"
            title={live ? "Entity name" : "Entity name — edit and press enter"}
          />
        </div>
        {id !== "" && id != null ? <span className="eui-id-badge">#{id}</span> : null}
        <button
          className={"eui-btn" + (interOpen ? " active" : "")}
          style={{ padding: "0 8px", fontSize: 12, flex: "none" }}
          title="Make this item interactive — pick a trigger (click, press E) and what happens. No code."
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
          />
        )}

        <RealComponentCards
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

const COMPONENT_NAMES = [
  "Animator",
  "AudioSource",
  "Billboard",
  "GltfContainer",
  "NftShape",
  "PointerEvents",
  "TextShape",
  "VisibilityComponent",
];

export function DeAddComponentPicker({ onPick = undefined }: { onPick?: (name: string) => void }) {
  return (
    <div className="eui-pop">
      <input className="eui-input" placeholder="Add component…" defaultValue="" />
      <div className="eui-pop-list">
        {COMPONENT_NAMES.map((n) => (
          <div
            key={n}
            className="eui-pop-item"
            role={onPick ? "button" : undefined}
            tabIndex={onPick ? 0 : undefined}
            onClick={onPick ? () => onPick(n) : undefined}
            onKeyDown={
              onPick
                ? (e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onPick(n);
                    }
                  }
                : undefined
            }
          >
            {n}
          </div>
        ))}
      </div>
    </div>
  );
}
