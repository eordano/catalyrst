import { createRequire } from 'node:module';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const scene = fileURLToPath(new URL('../../../bevy-explorer/editor-scene/', import.meta.url));
const require = createRequire(join(scene, 'package.json'));
const { buildSync } = createRequire(require.resolve('@dcl/sdk-commands/package.json'))('esbuild');
const dir = mkdtempSync(join(tmpdir(), 'authoring-schemas-'));
const output = new URL('../src/editor/generated/authoring-schemas.json', import.meta.url);
try {
  const file = join(dir, 'schemas.cjs');
  buildSync({ stdin: { resolveDir: scene, contents: `
    import { writeSync } from 'node:fs';
    import { ActionSchemas, createComponents } from '@dcl/asset-packs/dist/definitions';
    import * as ecs from '@dcl/ecs';
    import { engine } from '@dcl/sdk/ecs';
    const record = component => { let defaults; try { defaults = component.create() } catch { defaults = {} } return { schema: component.jsonSchema, defaults } };
    const components = {};
    for (const component of [...Object.values(ecs), ...Object.values(createComponents(engine))]) {
      if (component && component.componentName && component.schema?.jsonSchema) components[component.componentName] = { ...record(component.schema), componentId: component.componentId };
    }
    writeSync(1, JSON.stringify({ actions: Object.fromEntries(Object.entries(ActionSchemas).map(([name, schema]) => [name, record(schema)])), components }));
  ` }, outfile: file, bundle: true, platform: 'node', format: 'cjs', logLevel: 'warning' });
  const result = spawnSync(process.execPath, [file], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
  if (result.status !== 0) throw new Error(result.stderr || 'Schema extraction failed');
  const parsed = JSON.parse(result.stdout);
  const protobuf = require('protobufjs');
  const protoBase = join(dirname(require.resolve('@dcl/protocol/package.json')), 'proto');
  const root = protobuf.Root.fromJSON(require('protobufjs/google/protobuf/descriptor.json'));
  root.resolvePath = (_origin, target) => join(protoBase, target);
  const componentsDir = 'decentraland/sdk/components';
  root.loadSync(readdirSync(join(protoBase, componentsDir)).filter(name => name.endsWith('.proto')).map(name => `${componentsDir}/${name}`)).resolveAll();
  function messageSchema(type, seen = new Set()) {
    if (seen.has(type.fullName)) return { type: 'object', properties: {} };
    const next = new Set([...seen, type.fullName]);
    const properties = {};
    for (const field of type.fieldsArray) {
      if (field.partOf && !field.options?.proto3_optional) continue;
      properties[field.name] = fieldSchema(field, next);
    }
    for (const group of type.oneofsArray) {
      if (group.fieldsArray.every(field => field.options?.proto3_optional)) continue;
      properties[group.name] = { serializationType: 'optional', oneOf: group.fieldsArray.map(field => ({ type: 'object', properties: { [field.name]: fieldSchema(field, next) }, required: [field.name] })) };
    }
    return { type: 'object', properties };
  }
  function fieldSchema(field, seen) {
    const resolved = field.resolvedType;
    let schema;
    if (resolved instanceof protobuf.Enum) schema = { type: 'integer', enum: Object.values(resolved.values), enumObject: resolved.values };
    else if (resolved instanceof protobuf.Type) schema = messageSchema(resolved, seen);
    else schema = { type: field.type === 'string' || field.type === 'bytes' ? 'string' : field.type === 'bool' ? 'boolean' : ['float', 'double'].includes(field.type) ? 'number' : 'integer' };
    if (field.map) schema = { type: 'object', additionalProperties: schema };
    else if (field.repeated) schema = { type: 'array', items: schema };
    if (field.options?.proto3_optional) schema = { ...schema, serializationType: 'optional' };
    return schema;
  }
  for (const component of Object.values(parsed.components)) {
    if (component.schema.protocolBuffer) component.schema = messageSchema(root.lookupType(`decentraland.sdk.components.${component.schema.protocolBuffer}`));
  }
  function normalize(schema) {
    if (!schema || typeof schema !== 'object') return schema;
    if (Array.isArray(schema)) return schema.map(normalize);
    if (schema.optionalJsonSchema) return { ...normalize(schema.optionalJsonSchema), serializationType: 'optional' };
    const result = Object.fromEntries(Object.entries(schema).map(([key, value]) => [key, normalize(value)]));
    if (result.serializationType === 'map' && result.properties && !Object.keys(result.properties).length) result.additionalProperties = {};
    if (result.serializationType === 'vector3' && result.properties) delete result.properties.w;
    return result;
  }
  for (const definition of [...Object.values(parsed.actions), ...Object.values(parsed.components)]) definition.schema = normalize(definition.schema);
  parsed.versions = { sdk: require('@dcl/sdk/package.json').version, assetPacks: require('@dcl/asset-packs/package.json').version };
  const content = JSON.stringify(parsed, null, 2) + '\n';
  if (process.argv.includes('--check')) {
    if (readFileSync(output, 'utf8') !== content) throw new Error('Authoring schemas changed. Run npm run gen:authoring-schemas.');
  } else writeFileSync(output, content);
  console.log(`${Object.keys(parsed.actions).length} actions, ${Object.keys(parsed.components).length} components extracted from upstream schemas`);
} finally { rmSync(dir, { recursive: true, force: true }); }
