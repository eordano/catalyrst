import { ACTION_SCHEMAS, COMPONENT_SCHEMAS, fieldLabel } from "../authoring-schema";
import { DeSchemaFields } from "./DeSchemaFields";
import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { IconPlus, IconTrash } from "./DeIcons";
import { NumField, PropRow, TextField, type CompValue } from "./DeInspectorFields";

export const ACTION_TYPE_OPTIONS: readonly string[] = Object.keys(ACTION_SCHEMAS);
const triggerSchema = COMPONENT_SCHEMAS["asset-packs::Triggers"]!.schema.properties!.value!.items!;
const TRIGGER_TYPE_OPTIONS = (triggerSchema.properties!.type!.enum ?? []).map(value => ({ value: String(value), label: fieldLabel(String(value)) }));
const CONDITION_TYPE_OPTIONS = (triggerSchema.properties!.conditions!.items!.properties!.type!.enum ?? []).map(String);

export function entryList(v: CompValue, key = "value"): Record<string, unknown>[] {
  const val = v[key];
  if (!Array.isArray(val)) return [];
  return val.filter((e): e is Record<string, unknown> => e !== null && typeof e === "object");
}

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

export function EntryListShell({ title, emptyHint, count, addLabel, disabled, onAdd, children }: EntryListShellProps) {
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

export function ActionEntry({ entry, uid, readonly, onPatch, onRemove }: ActionEntryProps) {
  const name = typeof entry.name === "string" ? entry.name : "";
  const type = typeof entry.type === "string" ? entry.type : "";
  const setField = (k: string, val: unknown) => onPatch({ ...entry, [k]: val });
  const setF = readonly ? undefined : (val: unknown) => setField("name", val);
  const setTypeF = readonly ? undefined : (val: unknown) => onPatch({ ...entry, type: val, jsonPayload: JSON.stringify(ACTION_SCHEMAS[String(val)]?.defaults ?? {}) });
  const definition = ACTION_SCHEMAS[type];
  let payload: unknown = {};
  try { payload = typeof entry.jsonPayload === "string" ? JSON.parse(entry.jsonPayload) : entry.jsonPayload ?? {}; } catch { }
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
      {definition ? <DeSchemaFields schema={definition.schema} value={payload} onChange={readonly ? undefined : value => setField("jsonPayload", JSON.stringify(value))} /> : <PayloadField
        id={uid + "-payload"}
        raw={entry["jsonPayload"]}
        onCommit={readonly ? undefined : (s) => setField("jsonPayload", s)}
      />}

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

export function TriggerEntry(props: TriggerEntryProps) {
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
