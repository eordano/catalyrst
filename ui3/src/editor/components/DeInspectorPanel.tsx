import type { ReactNode } from "react";
import { useEffect, useId, useMemo, useState } from "react";
import Toggle from "../../atoms/Toggle";
import type {
  AuthorComponentFn,
  DeleteComponentFn,
  EditorTransform,
  EditorVec,
} from "../types";
import { nudgeFromKey } from "../transform-nudge";
import Modal from "../../components/Modal";
import DeInteractionsPanel, { type DeInteractionsPreset } from "./DeInteractionsPanel";
import { IconBolt, IconPlus, IconTrash } from "./DeIcons";
import { useOneShot } from "../use-one-shot";

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
              title={onNudge ? "\u{2191}/\u{2193} nudge \u{B1}1 \u{B7} shift \u{B1}0.01" : "drag to scrub \u{B7} shift for fine"}
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

export type NudgeFieldFn = (
  field: "position" | "rotation" | "scale",
  axis: keyof EditorVec,
  delta: number,
) => void;

type CompValue = Record<string, unknown>;
type WriteComp = (next: CompValue) => void;

function atPath(v: unknown, path: string[]): unknown {
  let cur: unknown = v;
  for (const k of path) {
    if (cur === null || typeof cur !== "object") return undefined;
    cur = (cur as Record<string, unknown>)[k];
  }
  return cur;
}

/** Immutably set a nested key, creating plain objects on the way down. */
function withPath(v: CompValue, path: string[], leaf: unknown): CompValue {
  if (path.length === 0) return v;
  const head = path[0];
  if (head === undefined) return v;
  const rest = path.slice(1);
  const child = v[head];
  const base = child !== null && typeof child === "object" ? (child as CompValue) : {};
  return { ...v, [head]: rest.length === 0 ? leaf : withPath(base, rest, leaf) };
}

/** SDK colours are 0..1 floats; <input type=color> speaks #rrggbb. */
function rgbToHex(c: unknown): string {
  const o = c !== null && typeof c === "object" ? (c as Record<string, unknown>) : {};
  const ch = (k: string) => {
    const n = o[k];
    const f = typeof n === "number" ? n : 1;
    return Math.max(0, Math.min(255, Math.round(f * 255)))
      .toString(16)
      .padStart(2, "0");
  };
  return `#${ch("r")}${ch("g")}${ch("b")}`;
}

function hexToRgb(hex: string, a: number): Record<string, number> {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m || m[1] === undefined) return { r: 1, g: 1, b: 1, a };
  const n = parseInt(m[1], 16);
  return {
    r: ((n >> 16) & 255) / 255,
    g: ((n >> 8) & 255) / 255,
    b: (n & 255) / 255,
    a,
  };
}

function NumField({
  id,
  value,
  onCommit,
}: {
  id: string;
  value: number;
  onCommit?: (n: number) => void;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const n = Number(draft);
    if (Number.isFinite(n) && n !== value) onCommit?.(n);
    else setDraft(String(value));
  };
  return (
    <input
      id={id}
      className="eui-num"
      value={draft}
      readOnly={onCommit === undefined}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setDraft(String(value));
      }}
    />
  );
}

function TextField({
  id,
  value,
  placeholder,
  onCommit,
  list,
}: {
  id: string;
  value: string;
  placeholder?: string;
  onCommit?: (s: string) => void;
  /** Optional datalist element id to suggest values without restricting input. */
  list?: string;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <input
      id={id}
      className="eui-input"
      value={draft}
      placeholder={placeholder}
      spellCheck={false}
      list={list}
      readOnly={onCommit === undefined}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => draft !== value && onCommit?.(draft)}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        if (e.key === "Escape") setDraft(value);
      }}
    />
  );
}

function BoolField({
  checked,
  label,
  onCommit,
}: {
  checked: boolean;
  label: string;
  onCommit?: (b: boolean) => void;
}) {
  return (
    <Toggle
      checked={checked}
      ariaLabel={label}
      disabled={onCommit === undefined}
      onChange={(next) => onCommit?.(next)}
    />
  );
}

// ---------------------------------------------------------------------------
// asset-packs::Actions / Triggers ("smart items") inline forms.
//
// Shapes come from the vendored component registry and a real composite:
//   asset-packs::Actions  -> { id?: int, value: ActionEntry[] }
//     ActionEntry = { name, type, jsonPayload /* JSON *string* */,
//                     allowedInBasicView?, basicViewId?, default? }
//   asset-packs::Triggers -> { value: TriggerEntry[] }
//     TriggerEntry = { type, conditions?: [{ id?, type, value /* string */ }],
//                      operation?: "and"|"or", actions: [{ id?, name? }],
//                      basicViewId? }
// jsonPayload is a STRING containing JSON; Triggers.actions reference actions BY
// NAME. Primitive leaf fields render as real inputs here; the payload object and
// anything we cannot type stays a per-entry JSON textarea (validated on blur).
// ---------------------------------------------------------------------------

const ACTION_TYPE_OPTIONS: readonly string[] = [
  "play_animation", "stop_animation", "set_state", "start_tween", "set_counter",
  "increment_counter", "decrease_counter", "play_sound", "stop_sound", "set_visibility",
  "attach_to_player", "detach_from_player", "play_video_stream", "stop_video_stream",
  "play_audio_stream", "stop_audio_stream", "teleport_player", "move_player",
  "play_default_emote", "play_custom_emote", "open_link", "show_text", "hide_text",
  "start_delay", "stop_delay", "start_loop", "stop_loop", "clone_entity", "remove_entity",
  "show_image", "hide_image", "damage", "move_player_here", "player_face_item",
  "place_on_player", "rotate_as_player", "place_on_camera", "rotate_as_camera",
  "set_position", "set_rotation", "set_scale", "follow_player", "stop_following_player",
  "random", "batch", "heal_player", "claim_airdrop", "lights_on", "lights_off",
  "lights_modify", "change_camera", "change_text", "stop_tween", "slide_texture",
  "freeze_player", "unfreeze_player", "change_collisions", "change_skybox",
  "reset_skybox", "call_script_method", "log_to_console", "delete",
];

// The closed TriggerType enum (component-schemas.json + gen-vocab) -- a select.
const TRIGGER_TYPE_OPTIONS: readonly { value: string; label: string }[] = [
  { value: "on_click", label: "on_click (item clicked)" },
  { value: "on_input_action", label: "on_input_action (E pressed)" },
  { value: "on_state_change", label: "on_state_change" },
  { value: "on_spawn", label: "on_spawn" },
  { value: "on_tween_end", label: "on_tween_end" },
  { value: "on_counter_change", label: "on_counter_change" },
  { value: "on_player_enters_area", label: "on_player_enters_area" },
  { value: "on_player_leaves_area", label: "on_player_leaves_area" },
  { value: "on_delay", label: "on_delay" },
  { value: "on_loop", label: "on_loop" },
  { value: "on_clone", label: "on_clone" },
  { value: "on_click_image", label: "on_click_image" },
  { value: "on_damage", label: "on_damage" },
  { value: "on_global_click", label: "on_global_click" },
  { value: "on_global_primary", label: "on_global_primary" },
  { value: "on_global_secondary", label: "on_global_secondary" },
  { value: "on_tick", label: "on_tick" },
  { value: "on_heal_player", label: "on_heal_player" },
  { value: "on_player_spawn", label: "on_player_spawn" },
];

const CONDITION_TYPE_OPTIONS: readonly string[] = [
  "when_state_is", "when_state_is_not", "when_counter_equals", "when_counter_is_greater_than",
  "when_counter_is_less_than", "when_distance_to_player_less_than",
  "when_distance_to_player_greater_than", "when_previous_state_is",
  "when_previous_state_is_not",
];

/** Read the `value: entry[]` array a smart-item component carries (default key `value`). */
function entryList(v: CompValue, key = "value"): Record<string, unknown>[] {
  const val = v[key];
  if (!Array.isArray(val)) return [];
  return val.filter((e): e is Record<string, unknown> => e !== null && typeof e === "object");
}

/** Canonical pretty-print of a jsonPayload that may arrive as a string or an object. */
function payloadText(raw: unknown): string {
  if (typeof raw === "string") {
    try {
      return JSON.stringify(JSON.parse(raw), null, 2);
    } catch {
      return raw;
    }
  }
  if (raw !== null && typeof raw === "object") return JSON.stringify(raw, null, 2);
  return "{}";
}

/**
 * Editable JSON box for one payload object. Validates with JSON.parse on blur;
 * an invalid edit keeps the old value and shows the standard error styling used
 * by the Edit-as-JSON modal. Commits a compact single-line string (the stored
 * representation) and only when the value actually changed.
 */
function PayloadField({
  id,
  raw,
  onCommit,
}: {
  id: string;
  raw: unknown;
  onCommit?: (s: string) => void;
}) {
  const canonical = payloadText(raw);
  const [draft, setDraft] = useState(canonical);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setDraft(canonical);
    setError(null);
  }, [canonical]);
  const readOnly = onCommit === undefined;
  const commit = () => {
    if (readOnly) return;
    let parsed: unknown;
    try {
      parsed = JSON.parse(draft);
    } catch (e) {
      setError("Invalid JSON \u{2014} " + (e instanceof Error ? e.message : "parse error"));
      setDraft(canonical);
      return;
    }
    setError(null);
    const next = JSON.stringify(parsed);
    // No-op guard: do not re-author an unchanged component (avoids a bus write
    // for the exact value the engine already has).
    if (next.replace(/\s/g, "") === canonical.replace(/\s/g, "")) return;
    onCommit(next);
  };
  return (
    <PropRow label="payload" htmlFor={id}>
      <span className="pvalue" style={{ flexDirection: "column", alignItems: "stretch", width: "100%" }}>
        <textarea
          id={id}
          className="eui-input"
          spellCheck={false}
          rows={3}
          readOnly={readOnly}
          value={draft}
          onChange={(e) => {
            setDraft(e.target.value);
            if (error) setError(null);
          }}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) commit();
            if (e.key === "Escape") {
              setDraft(canonical);
              setError(null);
            }
          }}
          style={{ width: "100%", fontFamily: "monospace", fontSize: 11, resize: "vertical", minHeight: 54 }}
        />
        {error && (
          <p role="alert" style={{ color: "var(--error, #e5484d)", fontSize: 11, margin: "4px 0 0" }}>
            {error}
          </p>
        )}
      </span>
    </PropRow>
  );
}

interface EntryListShellProps {
  title: string;
  emptyHint: string;
  count: number;
  addLabel: string;
  disabled?: boolean;
  onAdd: () => void;
  children?: ReactNode;
}

/** Group header + one row per entry + the add button, reused by both components. */
function EntryListShell({ title, emptyHint, count, addLabel, disabled, onAdd, children }: EntryListShellProps) {
  return (
    <>
      <div className="eui-group-label">{title}</div>
      {count === 0 ? <div className="eui-comp-note">{emptyHint}</div> : null}
      {children}
      <div className="eui-prop">
        <span className="plabel" />
        <span className="pvalue" style={{ display: "flex", justifyContent: "flex-start" }}>
          <button
            className="eui-btn"
            style={{ height: 24 }}
            disabled={disabled}
            onClick={onAdd}
          >
            <span style={{ marginRight: 4 }}><IconPlus /></span>
            {addLabel}
          </button>
        </span>
      </div>
    </>
  );
}

interface ActionEntryProps {
  entry: Record<string, unknown>;
  uid: string;
  readonly: boolean;
  onPatch: (next: Record<string, unknown>) => void;
  onRemove: () => void;
}

function ActionEntry({ entry, uid, readonly, onPatch, onRemove }: ActionEntryProps) {
  const name = typeof entry.name === "string" ? entry.name : "";
  const type = typeof entry.type === "string" ? entry.type : "";
  const setField = (k: string, val: unknown) => onPatch({ ...entry, [k]: val });
  const setF = readonly ? undefined : (val: unknown) => setField("name", val);
  const setTypeF = readonly ? undefined : (val: unknown) => setField("type", val);
  return (
    <div className="eui-group">
      <div className="eui-prop">
        <span className="plabel">name</span>
        <span className="pvalue">
          <TextField
            id={uid + "-name"}
            value={name}
            placeholder="Action name"
            list={uid + "-names"}
            onCommit={setF}
          />
          <button
            className="eui-btn icon"
            style={{ width: 20, height: 20 }}
            title="Remove action"
            aria-label="Remove action"
            disabled={readonly}
            onClick={onRemove}
          >
            <IconTrash />
          </button>
        </span>
      </div>
      <PropRow label="type" htmlFor={uid + "-type"}>
        <TextField
          id={uid + "-type"}
          value={type}
          placeholder="set_visibility"
          list={uid + "-types"}
          onCommit={setTypeF}
        />
      </PropRow>
      <PayloadField
        id={uid + "-payload"}
        raw={entry["jsonPayload"]}
        onCommit={readonly ? undefined : (s) => setField("jsonPayload", s)}
      />
    </div>
  );
}

interface ConditionItemProps {
  cond: Record<string, unknown>;
  uid: string;
  readonly: boolean;
  onPatch: (next: Record<string, unknown>) => void;
  onRemove: () => void;
}

function ConditionItem({ cond, uid, readonly, onPatch, onRemove }: ConditionItemProps) {
  const ctype = typeof cond.type === "string" ? cond.type : "";
  const value = typeof cond.value === "string" ? cond.value : "";
  const cid = cond.id;
  const numId = typeof cid === "number" ? cid : 0;
  const known = CONDITION_TYPE_OPTIONS.slice();
  const op = (k: string, val: unknown) => onPatch({ ...cond, [k]: val });
  return (
    <div style={{ paddingLeft: 4 }}>
      <div className="eui-prop">
        <span className="plabel">condition</span>
        <span className="pvalue">
          <select
            className="eui-select"
            aria-label="Condition type"
            value={ctype}
            disabled={readonly}
            onChange={(e) => op("type", e.target.value)}
          >
            {known.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
            {ctype && !known.includes(ctype) ? <option value={ctype}>{ctype}</option> : null}
          </select>
        </span>
      </div>
      <PropRow label="value" htmlFor={uid + "-cval"}>
        <TextField
          id={uid + "-cval"}
          value={value}
          placeholder="__state__ / number (as string)"
          onCommit={readonly ? undefined : (s) => op("value", s)}
        />
      </PropRow>
      <PropRow label="entity id" htmlFor={uid + "-cid"}>
        <span className="eui-axis">
          <span className="ax">N</span>
          <NumField
            id={uid + "-cid"}
            value={numId}
            onCommit={readonly ? undefined : (n) => op("id", Math.trunc(n))}
          />
        </span>
      </PropRow>
      <div className="eui-prop">
        <span className="plabel" />
        <span className="pvalue" style={{ display: "flex", justifyContent: "flex-start" }}>
          <button
            className="eui-btn"
            style={{ height: 22, fontSize: 11 }}
            disabled={readonly}
            onClick={onRemove}
          >
            <span style={{ marginRight: 4 }}><IconTrash /></span>
            remove condition
          </button>
        </span>
      </div>
    </div>
  );
}

interface RefActionItemProps {
  ref: Record<string, unknown>;
  uid: string;
  readonly: boolean;
  onPatch: (next: Record<string, unknown>) => void;
  onRemove: () => void;
}

function RefActionItem({ ref: refEntry, uid, readonly, onPatch, onRemove }: RefActionItemProps) {
  const value = typeof refEntry.name === "string" ? refEntry.name : "";
  const rid = refEntry.id;
  const numId = typeof rid === "number" ? rid : 0;
  const op = (k: string, val: unknown) => onPatch({ ...refEntry, [k]: val });
  return (
    <div className="eui-prop">
      <span className="plabel">runs action</span>
      <span className="pvalue">
        <TextField
          id={uid + "-ref"}
          value={value}
          placeholder="Action name"
          list={uid + "-names"}
          onCommit={readonly ? undefined : (s) => op("name", s)}
        />
        <NumField
          id={uid + "-refid"}
          value={numId}
          onCommit={readonly ? undefined : (n) => op("id", Math.trunc(n))}
        />
        <button
          className="eui-btn icon"
          style={{ width: 20, height: 20 }}
          title="Remove reference"
          aria-label="Remove action reference"
          disabled={readonly}
          onClick={onRemove}
        >
          <IconTrash />
        </button>
      </span>
    </div>
  );
}

interface TriggerEntryProps {
  entry: Record<string, unknown>;
  uid: string;
  readonly: boolean;
  onPatch: (next: Record<string, unknown>) => void;
  onRemove: () => void;
}

function TriggerEntry(props: TriggerEntryProps) {
  const { entry, uid, readonly, onPatch, onRemove } = props;
  const type = typeof entry.type === "string" ? entry.type : "";
  const known = TRIGGER_TYPE_OPTIONS.filter((t) => t.value).map((t) => t.value as string);
  const conditions = entryList(entry, "conditions");
  const refActions = entryList(entry, "actions");
  const operation = typeof entry.operation === "string" ? entry.operation : "";
  const condOp = (i: number, next: Record<string, unknown>) =>
    onPatch({ ...entry, conditions: conditions.map((c, j) => (j === i ? next : c)) });
  const refOp = (i: number, next: Record<string, unknown>) =>
    onPatch({ ...entry, actions: refActions.map((r, j) => (j === i ? next : r)) });
  return (
    <div className="eui-group">
      <div className="eui-prop">
        <span className="plabel">when</span>
        <span className="pvalue">
          <select
            className="eui-select"
            aria-label="Trigger type"
            value={type}
            disabled={readonly}
            style={{ flex: 1 }}
            onChange={(e) => onPatch({ ...entry, type: e.target.value })}
          >
            {TRIGGER_TYPE_OPTIONS.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
            {type && !known.includes(type) ? <option value={type}>{type}</option> : null}
          </select>
          <button
            className="eui-btn icon"
            style={{ width: 20, height: 20 }}
            title="Remove trigger"
            aria-label="Remove trigger"
            disabled={readonly}
            onClick={onRemove}
          >
            <IconTrash />
          </button>
        </span>
      </div>

      {conditions.length > 0 && (
        <>
          <div className="eui-group-label">conditions {operation ? "(" + operation + ")" : ""}</div>
          {conditions.map((c, i) => (
            <ConditionItem
              key={i}
              cond={c}
              uid={uid + "-c" + i}
              readonly={readonly}
              onPatch={(next) => condOp(i, next)}
              onRemove={() => onPatch({ ...entry, conditions: conditions.filter((_, j) => j !== i) })}
            />
          ))}
          {conditions.length >= 2 && (
            <PropRow label="combine" htmlFor={uid + "-op"}>
              <select
                id={uid + "-op"}
                className="eui-select"
                value={operation || "and"}
                disabled={readonly}
                onChange={(e) => onPatch({ ...entry, operation: e.target.value })}
              >
                <option value="and">and (all must hold)</option>
                <option value="or">or (any may hold)</option>
              </select>
            </PropRow>
          )}
          <div className="eui-prop">
            <span className="plabel" />
            <span className="pvalue" style={{ display: "flex", justifyContent: "flex-start" }}>
              <button
                className="eui-btn"
                style={{ height: 22, fontSize: 11 }}
                disabled={readonly}
                onClick={() => onPatch({ ...entry, conditions: [...conditions, { type: "when_state_is", value: "" }] })}
              >
                <span style={{ marginRight: 4 }}><IconPlus /></span>
                add condition
              </button>
            </span>
          </div>
        </>
      )}

      <div className="eui-group-label">runs</div>
      {refActions.length === 0 && <div className="eui-comp-note">No actions referenced yet.</div>}
      {refActions.map((r, i) => (
        <RefActionItem
          key={i}
          ref={r}
          uid={uid + "-r" + i}
          readonly={readonly}
          onPatch={(next) => refOp(i, next)}
          onRemove={() => onPatch({ ...entry, actions: refActions.filter((_, j) => j !== i) })}
        />
      ))}
      <div className="eui-prop">
        <span className="plabel" />
        <span className="pvalue" style={{ display: "flex", justifyContent: "flex-start" }}>
          <button
            className="eui-btn"
            style={{ height: 22, fontSize: 11 }}
            disabled={readonly}
            onClick={() => onPatch({ ...entry, actions: [...refActions, { name: "" }] })}
          >
            <span style={{ marginRight: 4 }}><IconPlus /></span>
            add action reference
          </button>
        </span>
      </div>
    </div>
  );
}

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
  // One select for both mesh oneofs; `keep` is what survives a shape switch --
  // the collider keeps its sibling fields (collisionMask), the renderer starts
  // the value fresh because its shape payloads do not overlap.
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
      // Triggers reference actions BY NAME -- suggest the sibling Actions' names.
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
  /** Live component values by name, so the fields show the scene rather than defaults. */
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

export interface DeInspectorPanelProps {
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
  /** Bumped by the host to re-open a section the user has since closed. */
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
  const addPickerOpen = addOpen || localAddOpen;
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
  /** Engine component id -- emitted verbatim on the bus, never localized. */
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

export function DeAddComponentPicker({ onPick = undefined }: { onPick?: (name: string) => void }) {
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
