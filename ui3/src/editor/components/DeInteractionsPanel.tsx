import { ACTION_SCHEMAS, COMPONENT_SCHEMAS, fieldLabel, schemaError, type AuthoringSchema } from "../authoring-schema";
import { DeSchemaFields } from "./DeSchemaFields";
import type { ReactNode } from "react";
import { useId, useMemo, useState } from "react";
import Toggle from "../../atoms/Toggle";
import { useOneShot } from "../use-one-shot";

type FieldKind = "num" | "bool" | "text";

interface ActionField {
  key: string;
  label: string;
  kind: FieldKind;
  def: number | boolean | string;
  placeholder?: string;
  required?: boolean;
}

type FieldValues = Record<string, string | number | boolean>;

interface ActionDef {
  schema?: AuthoringSchema;
  defaults?: unknown;
  id: string;
  label: string;
  hint: string;
  fields: ActionField[];
  compose: (f: FieldValues) => Record<string, unknown>;
}

interface TriggerDef {
  id: string;
  label: string;
}

interface ExistingActions {
  id?: number;
  value?: unknown[];
}
interface ExistingTriggers {
  value?: unknown[];
}

function actionsIdFor(entityId: string | number | null | undefined): number {
  const n = parseInt(String(entityId ?? ""), 10);
  return Number.isFinite(n) && n > 0 ? n : 1;
}

function num(v: unknown, fallback: number): number {
  const n = typeof v === "number" ? v : parseFloat(String(v));
  return Number.isFinite(n) ? n : fallback;
}

const SIMPLE_TRIGGERS: TriggerDef[] = [
  { id: "on_click", label: "Item is clicked" },
  { id: "on_input_action", label: "Primary button is pressed (E)" },
];

const triggerSchema = COMPONENT_SCHEMAS["asset-packs::Triggers"]!.schema.properties!.value!.items!;
export const TRIGGERS: TriggerDef[] = [
  ...SIMPLE_TRIGGERS,
  ...(triggerSchema.properties!.type!.enum ?? []).map(String).filter(id => !SIMPLE_TRIGGERS.some(trigger => trigger.id === id)).map(id => ({ id, label: fieldLabel(id) })),
];
const triggerDetailsSchema: AuthoringSchema = { type: "object", properties: Object.fromEntries(Object.entries(triggerSchema.properties ?? {}).filter(([key]) => key !== "type" && key !== "actions")) };

const SIMPLE_ACTIONS: ActionDef[] = [
  {
    id: "start_tween",
    label: "Move the item",
    hint: "Smoothly tween the item to a new position",
    fields: [
      { key: "x", label: "X", kind: "num", def: 0 },
      { key: "y", label: "Y", kind: "num", def: 1 },
      { key: "z", label: "Z", kind: "num", def: 0 },
      { key: "duration", label: "Duration (s)", kind: "num", def: 1 },
      { key: "relative", label: "Relative to current", kind: "bool", def: true },
    ],
    compose: (f) => ({
      type: "move_item",
      end: { x: num(f.x, 0), y: num(f.y, 0), z: num(f.z, 0) },
      interpolationType: "linear",
      duration: num(f.duration, 1),
      relative: !!f.relative,
    }),
  },
  {
    id: "set_visibility",
    label: "Show / hide the item",
    hint: "Toggle whether the item is visible",
    fields: [{ key: "visible", label: "Visible", kind: "bool", def: true }],
    compose: (f) => ({ visible: !!f.visible }),
  },
  {
    id: "play_sound",
    label: "Play a sound",
    hint: "Play an audio clip from the scene",
    fields: [
      { key: "src", label: "Sound file", kind: "text", def: "", placeholder: "sounds/click.mp3", required: true },
      { key: "volume", label: "Volume", kind: "num", def: 1 },
      { key: "loop", label: "Loop", kind: "bool", def: false },
    ],
    compose: (f) => ({ src: String(f.src ?? "").trim(), volume: num(f.volume, 1), loop: !!f.loop }),
  },
  {
    id: "play_animation",
    label: "Play an animation",
    hint: "Play a clip from the item's GLTF model",
    fields: [
      { key: "animation", label: "Clip name", kind: "text", def: "", placeholder: "Action", required: true },
      { key: "loop", label: "Loop", kind: "bool", def: false },
    ],
    compose: (f) => ({ animation: String(f.animation ?? "").trim(), loop: !!f.loop }),
  },
];

export const ACTIONS: ActionDef[] = [
  ...SIMPLE_ACTIONS,
  ...Object.entries(ACTION_SCHEMAS).filter(([id]) => !SIMPLE_ACTIONS.some(action => action.id === id)).map(([id, definition]) => ({
    id, label: fieldLabel(id), hint: "Configure the action below.", fields: [], schema: definition.schema, defaults: definition.defaults,
    compose: () => definition.defaults as Record<string, unknown>,
  })),
];

function defaultsFor(action: ActionDef): FieldValues {
  const out: FieldValues = {};
  for (const fld of action.fields) out[fld.key] = fld.def;
  return out;
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

interface FieldProps {
  field: ActionField;
  fieldId: string;
  value: string | number | boolean | undefined;
  onChange: (value: string | boolean) => void;
}

function Field({ field, fieldId, value, onChange }: FieldProps) {
  if (field.kind === "bool") {
    return (
      <PropRow label={field.label}>
        <Toggle checked={!!value} ariaLabel={field.label} onChange={(next) => onChange(next)} />
      </PropRow>
    );
  }
  if (field.kind === "num") {
    return (
      <PropRow label={field.label} htmlFor={fieldId}>
        <span className="eui-axis">
          <span className="ax">N</span>
          <input
            id={fieldId}
            className="eui-num"
            value={(value ?? "") as string | number}
            spellCheck={false}
            onChange={(e) => onChange(e.target.value)}
          />
        </span>
      </PropRow>
    );
  }
  return (
    <PropRow label={field.label} htmlFor={fieldId}>
      <input
        id={fieldId}
        className="eui-input"
        value={(value ?? "") as string | number}
        placeholder={field.placeholder || ""}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
      />
    </PropRow>
  );
}

interface AddedInteraction {
  trigger: string;
  action: string;
  actionsJson: string;
  triggersJson: string;
}

export interface DeInteractionsPreset {
  nonce: number;
  trigger?: string;
  action?: string;
}

interface DeInteractionsPanelProps {
  entityId?: string | number;
  entityName?: string | null;
  onWrite?: ((name: string, json: string) => void | Promise<void>) | null;
  onWriteBatch?: (changes: { name: string; json: string }[]) => Promise<void>;
  existingActions?: ExistingActions | null;
  existingTriggers?: ExistingTriggers | null;
  preset?: DeInteractionsPreset | null;
}

export default function DeInteractionsPanel({
  entityId = "0",
  entityName = null,
  onWrite = null,
  onWriteBatch,
  existingActions = null,
  existingTriggers = null,
  preset = null,
}: DeInteractionsPanelProps) {
  const uid = useId();
  const [triggerType, setTriggerType] = useState(TRIGGERS[0]!.id);
  const [actionId, setActionId] = useState(ACTIONS[0]!.id);
  const [fields, setFields] = useState<FieldValues>(() => defaultsFor(ACTIONS[0]!));
  const [name, setName] = useState("");
  const [payload, setPayload] = useState<unknown>({});
  const [triggerDetails, setTriggerDetails] = useState<unknown>({});
  const [added, setAdded] = useState<AddedInteraction[]>([]);
  const [status, setStatus] = useState("");
  const [pending, setPending] = useState(false);

  const action = useMemo(() => ACTIONS.find((a) => a.id === actionId) ?? ACTIONS[0]!, [actionId]);

  const pickAction = (id: string) => {
    const next = ACTIONS.find((a) => a.id === id) ?? ACTIONS[0]!;
    setActionId(next.id);
    setFields(defaultsFor(next));
    setPayload(structuredClone(next.defaults ?? {}));
    setStatus("");
  };

  useOneShot(preset?.nonce ?? 0, () => {
    if (!preset) return;
    if (preset.trigger && TRIGGERS.some((t) => t.id === preset.trigger)) setTriggerType(preset.trigger);
    if (preset.action && ACTIONS.some((a) => a.id === preset.action)) pickAction(preset.action);
  });

  const setField = (key: string, v: string | boolean) => setFields((prev) => ({ ...prev, [key]: v }));

  const compose = () => {
    const existingNames = new Set((existingActions?.value ?? []).map(value => value && typeof value === "object" ? (value as { name?: string }).name : undefined));
    let actionName = name.trim() || action.label;
    if (!name.trim()) {
      let suffix = 2;
      while (existingNames.has(actionName)) actionName = `${action.label} ${suffix++}`;
    }
    const baseActions = existingActions && typeof existingActions === "object" ? existingActions : null;
    const id = baseActions && Number.isFinite(baseActions.id) ? baseActions.id : actionsIdFor(entityId);
    const actionEntry = {
      name: actionName,
      type: action.id,
      jsonPayload: JSON.stringify(action.schema ? payload : action.compose(fields)),
    };
    const actionsValue = [...(baseActions && Array.isArray(baseActions.value) ? baseActions.value : []), actionEntry];
    const actionsJson = JSON.stringify({ id, value: actionsValue });

    const baseTriggers = existingTriggers && typeof existingTriggers === "object" ? existingTriggers : null;
    const triggerEntry = { ...(triggerDetails as Record<string, unknown>), type: triggerType, actions: [{ id, name: actionName }] };
    const triggersValue = [...(baseTriggers && Array.isArray(baseTriggers.value) ? baseTriggers.value : []), triggerEntry];
    const triggersJson = JSON.stringify({ value: triggersValue });

    return { actionName, actionsJson, triggersJson };
  };

  const validationError = (action.schema ? schemaError(action.schema, payload, "Action") : null) ?? schemaError(triggerDetailsSchema, triggerDetails, "Trigger");
  const valid = !validationError && action.fields.every((f) => {
    if (!f.required) return true;
    return String(fields[f.key] ?? "").trim() !== "";
  });

  const confirm = async () => {
    if (pending) return;
    if (!valid) {
      setStatus(validationError ?? "Fill the required field first");
      return;
    }
    const { actionName, actionsJson, triggersJson } = compose();
    setPending(true);
    try {
      if (onWriteBatch) await onWriteBatch([{ name: "asset-packs::Actions", json: actionsJson }, { name: "asset-packs::Triggers", json: triggersJson }]);
      else {
        await onWrite?.("asset-packs::Actions", actionsJson);
        await onWrite?.("asset-packs::Triggers", triggersJson);
      }
      const triggerLabel = TRIGGERS.find((t) => t.id === triggerType)?.label ?? triggerType;
      setAdded((prev) => [...prev, { trigger: triggerLabel, action: actionName, actionsJson, triggersJson }]);
      setStatus(onWrite || onWriteBatch ? "\u{2713} Interaction added" : "\u{2713} Composed (preview \u{2014} not wired)");
      setName("");
    } catch (e) {
      setStatus("Failed to author: " + String(e));
    } finally { setPending(false); }
  };

  const preview = useMemo(() => {
    const { actionsJson, triggersJson } = compose();
    return { actionsJson, triggersJson };
  }, [triggerType, actionId, fields, payload, triggerDetails, name, existingActions, existingTriggers, entityId]);

  return (
    <div className="eui-comp" style={{ borderColor: "var(--primary-border)" }}>
      <div className="eui-comp-head" style={{ cursor: "default" }}>
        <span className="name">
          <span className="ns">Smart Item / </span>
          Add interaction
        </span>
        <span className="spacer" />
        <span className="eui-id-badge" title="Authoring on this entity">#{entityId}</span>
      </div>
      <div className="eui-comp-body">
        <div className="eui-comp-note" style={{ marginTop: 0 }}>
          Make {entityName ? `\u{201C}${entityName}\u{201D}` : "this item"} interactive &#x2014; pick what
          happens and when. No code.
        </div>

        <div className="eui-group-label">when</div>
        <PropRow label="Trigger" htmlFor={uid + "-trigger"}>
          <select
            id={uid + "-trigger"}
            className="eui-select"
            value={triggerType}
            onChange={(e) => setTriggerType(e.target.value)}
          >
            {TRIGGERS.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}
              </option>
            ))}
          </select>
        </PropRow>

        <DeSchemaFields schema={triggerDetailsSchema} value={triggerDetails} onChange={setTriggerDetails} />

        <div className="eui-group-label">do</div>
        <PropRow label="Action" htmlFor={uid + "-action"}>
          <select id={uid + "-action"} className="eui-select" value={actionId} onChange={(e) => pickAction(e.target.value)}>
            {ACTIONS.map((a) => (
              <option key={a.id} value={a.id}>
                {a.label}
              </option>
            ))}
          </select>
        </PropRow>
        <div className="eui-comp-note" style={{ marginTop: 2 }}>{action.hint}</div>

        {action.schema && <DeSchemaFields schema={action.schema} value={payload} onChange={setPayload} />}
        {action.fields.map((f) => (
          <Field
            key={f.key}
            field={f}
            fieldId={uid + "-f-" + f.key}
            value={fields[f.key]}
            onChange={(v) => setField(f.key, v)}
          />
        ))}

        <PropRow label="Name" htmlFor={uid + "-name"}>
          <input
            id={uid + "-name"}
            className="eui-input"
            value={name}
            placeholder={action.label}
            spellCheck={false}
            onChange={(e) => setName(e.target.value)}
          />
        </PropRow>

        <div style={{ display: "flex", gap: 6, alignItems: "center", marginTop: 8 }}>
          <button className="eui-btn primary" style={{ height: 24 }} disabled={!valid || pending} onClick={() => void confirm()}>
            {pending ? "Adding interaction\u2026" : "Add interaction"}
          </button>
          {status !== "" && (
            <span
              className={"eui-comp-status " + (status.startsWith("\u{2713}") ? "ok" : "err")}
              style={{ margin: 0 }}
              role={status.startsWith("\u{2713}") ? "status" : "alert"}
            >
              {status}
            </span>
          )}
        </div>

        {added.length > 0 && (
          <div style={{ marginTop: 10 }}>
            <div className="eui-group-label">on this item</div>
            {added.map((a, i) => (
              <div key={i} className="eui-prop">
                <span className="plabel" style={{ color: "var(--text-2)" }}>
                  {a.trigger}
                </span>
                <span className="pvalue" style={{ justifyContent: "flex-start" }}>
                  &#x2192; {a.action}
                </span>
              </div>
            ))}
          </div>
        )}

        <details style={{ marginTop: 10 }}>
          <summary className="eui-link" style={{ cursor: "pointer" }}>
            component json
          </summary>
          <textarea
            className="eui-raw"
            readOnly
            aria-label="Component JSON preview"
            spellCheck={false}
            value={
              "asset-packs::Actions\n" +
              pretty(preview.actionsJson) +
              "\n\nasset-packs::Triggers\n" +
              pretty(preview.triggersJson)
            }
          />
        </details>
      </div>
    </div>
  );
}

function pretty(json: string): string {
  try {
    return JSON.stringify(JSON.parse(json), null, 2);
  } catch {
    return json;
  }
}
