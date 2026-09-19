import catalog from "./generated/authoring-schemas.json";

export interface AuthoringSchema {
  type?: string;
  title?: string;
  properties?: Record<string, AuthoringSchema>;
  additionalProperties?: AuthoringSchema;
  items?: AuthoringSchema;
  enum?: Array<string | number>;
  enumObject?: Record<string, string | number>;
  default?: unknown;
  required?: string[];
  oneOf?: AuthoringSchema[];
  serializationType?: string;
  optionalJsonSchema?: AuthoringSchema;
}
export interface AuthoringDefinition { schema: AuthoringSchema; defaults: unknown; componentId?: number }
export const ACTION_SCHEMAS = catalog.actions as Record<string, AuthoringDefinition>;
export const COMPONENT_SCHEMAS = catalog.components as unknown as Record<string, AuthoringDefinition>;
export const AUTHORING_VERSIONS = catalog.versions;

export function fieldLabel(value: string): string {
  const words = value.replace(/([a-z\d])([A-Z])/g, "$1 $2").replaceAll("_", " ").toLowerCase();
  return words.charAt(0).toUpperCase() + words.slice(1);
}
export function schemaDefault(schema: AuthoringSchema): unknown {
  if (schema.default !== undefined) return structuredClone(schema.default);
  if (schema.enum?.length) return schema.enum[0];
  if (schema.oneOf?.length) return schemaDefault(schema.oneOf[0]!);
  if (schema.type === "array") return [];
  if (schema.type === "object") return Object.fromEntries(Object.entries(schema.properties ?? {}).filter(([, child]) => child.serializationType !== "optional").map(([key, child]) => [key, schemaDefault(child)]));
  if (schema.type === "boolean") return false;
  if (schema.type === "number" || schema.type === "integer") return 0;
  return "";
}
export function schemaVariant(schema: AuthoringSchema, value: unknown): number {
  const object = value && typeof value === "object" ? value as Record<string, unknown> : {};
  const index = schema.oneOf?.findIndex(variant => (variant.required ?? Object.keys(variant.properties ?? {})).some(key => key in object)) ?? -1;
  return Math.max(0, index);
}
export function schemaError(schema: AuthoringSchema, value: unknown, label = "Value"): string | null {
  if (value === undefined && schema.serializationType === "optional") return null;
  if (schema.oneOf?.length) return schemaError(schema.oneOf[schemaVariant(schema, value)]!, value, label);
  if (schema.enum && !schema.enum.includes(value as string | number)) return `${label}: choose a supported value.`;
  if (schema.type === "integer" && !Number.isSafeInteger(value)) return `${label}: enter a whole number.`;
  if (schema.type === "number" && (typeof value !== "number" || !Number.isFinite(value))) return `${label}: enter a finite number.`;
  if (schema.type === "string" && typeof value !== "string") return `${label}: enter text.`;
  if (schema.type === "boolean" && typeof value !== "boolean") return `${label}: choose true or false.`;
  if (schema.type === "array") {
    if (!Array.isArray(value)) return `${label}: expected a list.`;
    for (const [index, entry] of value.entries()) { const error = schemaError(schema.items ?? {}, entry, `${label} ${index + 1}`); if (error) return error; }
  }
  if (schema.type === "object") {
    if (!value || typeof value !== "object" || Array.isArray(value)) return `${label}: expected fields.`;
    for (const [key, child] of Object.entries(schema.properties ?? {})) {
      const current = (value as Record<string, unknown>)[key];
      if (current === undefined && !schema.required?.includes(key)) continue;
      const error = schemaError(child, current, `${label} / ${fieldLabel(key)}`); if (error) return error;
    }
  }
  return null;
}

export function componentFields(name: string, value: Record<string, unknown>): AuthoringSchema | undefined {
  const definition = COMPONENT_SCHEMAS[name];
  if (!definition) return undefined;
  const schema = structuredClone(definition.schema);
  if (name === "asset-packs::States" && schema.properties) {
    schema.properties.value = { ...schema.properties.value, title: "States" };
    const states = Array.isArray(value.value) ? value.value.filter((entry): entry is string => typeof entry === "string") : [];
    for (const key of ["defaultValue", "currentValue", "previousValue"]) {
      if (states.length) schema.properties[key] = { ...schema.properties[key], enum: states };
    }
  }
  if (name === "core-schema::Sync-Components" && schema.properties?.componentIds) {
    const components = Object.entries(COMPONENT_SCHEMAS).filter(([, component]) => typeof component.componentId === "number");
    schema.properties.componentIds = { ...schema.properties.componentIds, title: "Synchronized components", items: {
      type: "integer", enum: components.map(([, component]) => component.componentId!),
      enumObject: Object.fromEntries(components.map(([componentName, component]) => [componentName.replace(/^.*::/, ""), component.componentId!])),
    } };
  }
  return schema;
}
