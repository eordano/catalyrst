import { useId, useState } from "react";
import { fieldLabel, schemaDefault, schemaVariant, type AuthoringSchema } from "../authoring-schema";
import { NumField, PropRow, TextField } from "./DeInspectorFields";

interface Props { schema: AuthoringSchema; value: unknown; onChange?: (value: unknown) => void; label?: string; depth?: number }
const record = (value: unknown): Record<string, unknown> => value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};

export function DeSchemaFields({ schema, value, onChange, label = "", depth = 0 }: Props) {
  const uid = useId();
  const [newKey, setNewKey] = useState("");
  const [newType, setNewType] = useState("string");
  if (depth > 12) return <p className="eui-comp-note">Nested fields exceed the supported editing depth.</p>;
  if (schema.serializationType === "optional" && value === undefined) return <PropRow label={label}>
    <button className="eui-btn" disabled={!onChange} onClick={() => onChange?.(schemaDefault(schema.optionalJsonSchema ?? schema))}>Set {label.toLowerCase()}</button>
  </PropRow>;
  const unset = schema.serializationType === "optional" ? <button className="eui-btn" disabled={!onChange} aria-label={`Reset ${label.toLowerCase()} to default`} onClick={() => onChange?.(undefined)}>Use default</button> : null;
  if (schema.oneOf?.length) {
    const selected = schemaVariant(schema, value);
    return <div className="eui-group">
      <PropRow label={label} htmlFor={uid}><select id={uid} className="eui-select" disabled={!onChange} value={selected} onChange={event => onChange?.(schemaDefault(schema.oneOf![Number(event.target.value)]!))}>
        {schema.oneOf.map((variant, index) => <option key={index} value={index}>{fieldLabel((variant.required ?? Object.keys(variant.properties ?? {}))[0] ?? `Variant ${index + 1}`)}</option>)}
      </select>{unset}</PropRow>
      <DeSchemaFields schema={schema.oneOf[selected]!} value={value} onChange={onChange} depth={depth + 1} />
    </div>;
  }
  if (schema.type === "object") {
    const object = record(value);
    const properties = schema.properties ?? {};
    const dynamic = !!schema.additionalProperties || (schema.serializationType === "map" && !Object.keys(properties).length);
    const keys = [...new Set([...Object.keys(properties), ...Object.keys(object).filter(key => key !== "$case")])];
    return <div className="eui-group">
      {label && <div className="eui-group-label">{label} {unset}</div>}
      {keys.map(key => <DeSchemaFields key={key} label={properties[key]?.title ?? fieldLabel(key)} schema={properties[key] ?? (schema.additionalProperties?.type ? schema.additionalProperties : inferSchema(object[key]))} value={object[key]} depth={depth + 1} onChange={onChange ? next => {
        const result = { ...object };
        if (next === undefined) delete result[key]; else result[key] = next;
        onChange(result);
      } : undefined} />)}
      {dynamic && <div className="eui-prop">
        <input className="eui-input" aria-label={`${label || "Object"} field name`} placeholder="Field name" value={newKey} disabled={!onChange} onChange={event => setNewKey(event.target.value)} />
        <select className="eui-select" aria-label={`${label || "Object"} field type`} value={newType} disabled={!onChange} onChange={event => setNewType(event.target.value)}><option value="string">Text</option><option value="number">Number</option><option value="boolean">Boolean</option><option value="object">Object</option><option value="array">List</option></select>
        <button className="eui-btn" disabled={!onChange || !newKey.trim() || newKey.trim() in object || ["__proto__", "constructor", "prototype"].includes(newKey.trim())} onClick={() => { onChange?.({ ...object, [newKey.trim()]: schemaDefault(schema.additionalProperties?.type ? schema.additionalProperties : { type: newType }) }); setNewKey(""); }}>Add field</button>
      </div>}
    </div>;
  }
  if (schema.type === "array") {
    const entries = Array.isArray(value) ? value : [];
    return <div className="eui-group">
      <div className="eui-group-label">{label} {unset}</div>
      {entries.map((entry, index) => <div key={index} className="eui-group">
        <DeSchemaFields label={`${label} ${index + 1}`} schema={schema.items ?? inferSchema(entry)} value={entry} depth={depth + 1} onChange={onChange ? next => onChange(entries.map((old, i) => i === index ? next : old)) : undefined} />
        <button className="eui-btn" disabled={!onChange} aria-label={`Remove ${label.toLowerCase()} ${index + 1}`} onClick={() => onChange?.(entries.filter((_, i) => i !== index))}>Remove</button>
      </div>)}
      <button className="eui-btn" disabled={!onChange} onClick={() => onChange?.([...entries, schemaDefault(schema.items ?? { type: "string" })])}>Add {label.toLowerCase() || "entry"}</button>
    </div>;
  }
  const choices = schema.enum;
  return <PropRow label={label} htmlFor={uid}>
    {choices ? <select id={uid} className="eui-select" disabled={!onChange} value={String(value ?? choices[0])} onChange={event => onChange?.(choices.find(choice => String(choice) === event.target.value))}>
      {choices.map(choice => <option key={String(choice)} value={String(choice)}>{fieldLabel(Object.entries(schema.enumObject ?? {}).find(([key, val]) => val === choice && !/^\d+$/.test(key))?.[0].replace(/^[A-Z]{2,5}_/, "") ?? String(choice))}</option>)}
    </select> : schema.type === "boolean" ? <input id={uid} type="checkbox" checked={value === true} disabled={!onChange} onChange={event => onChange?.(event.target.checked)} />
      : schema.type === "number" || schema.type === "integer" ? <NumField id={uid} value={typeof value === "number" ? value : 0} onCommit={onChange ? number => onChange(schema.type === "integer" ? Math.trunc(number) : number) : undefined} />
        : <TextField id={uid} value={typeof value === "string" ? value : ""} onCommit={onChange} />}
    {unset}
  </PropRow>;
}
function inferSchema(value: unknown): AuthoringSchema {
  return { type: Array.isArray(value) ? "array" : value && typeof value === "object" ? "object" : typeof value === "number" ? "number" : typeof value === "boolean" ? "boolean" : "string", serializationType: "optional" };
}
