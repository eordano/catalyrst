#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'

const ECS_DEFAULT = 'node_modules/@dcl/ecs'
const ecsRoot: string = process.env.DCL_ECS_PATH || ECS_DEFAULT
const outFile = process.argv[2]
if (!outFile) {
  console.error('usage: dump-composite-schema-table.mts <out-file.json>')
  process.exit(2)
}
const require = createRequire(import.meta.url)

// The resolved @dcl/ecs is whatever the caller's DCL_ECS_PATH/node_modules
// points at (no fixed version, no @types available at this dynamic
// resolution root), so its shape is genuinely opaque beyond the handful of
// members this script actually calls — captured narrowly below rather than
// pretending to know the whole package surface.
interface EcsComponentDefinition {
  componentName: string
  componentId: number
  componentType: unknown
  schema: { jsonSchema?: unknown }
}
interface EcsEngine {
  componentsIter(): Iterable<EcsComponentDefinition>
}
interface EcsModule {
  Engine(): EcsEngine
}
type ComponentFactory = (engine: EcsEngine) => unknown

const ecs = require(path.join(ecsRoot, 'dist-cjs')) as EcsModule
const gen = require(path.join(ecsRoot, 'dist-cjs/components/generated/index.gen.js')) as {
  componentDefinitionByName: Record<string, ComponentFactory>
}
const comps = require(path.join(ecsRoot, 'dist-cjs/components')) as Record<string, unknown>
const compositeComponents = require(path.join(ecsRoot, 'dist-cjs/composite/components')) as {
  getCompositeRootComponent: (engine: EcsEngine) => unknown
}
const ecsVersion: string = (require(path.join(ecsRoot, 'package.json')) as { version: string }).version

const engine = ecs.Engine()
compositeComponents.getCompositeRootComponent(engine)
const errors: string[] = []
for (const [name, factory] of Object.entries(gen.componentDefinitionByName)) {
  try {
    factory(engine)
  } catch (err) {
    errors.push(`${name}: ${(err as Error).message}`)
  }
}
for (const [name, factory] of Object.entries(comps)) {
  if (typeof factory !== 'function') continue
  try {
    ;(factory as ComponentFactory)(engine)
  } catch (err) {
    errors.push(`components.${name}: ${(err as Error).message}`)
  }
}

interface ComponentTableRow {
  name: string
  componentId: number
  componentType: unknown
  inStaticTable: boolean
  jsonSchema: unknown
}

const table: ComponentTableRow[] = []
for (const def of engine.componentsIter()) {
  table.push({
    name: def.componentName,
    componentId: def.componentId,
    componentType: def.componentType,
    inStaticTable: def.componentName in gen.componentDefinitionByName,
    jsonSchema: def.schema.jsonSchema ?? null
  })
}
table.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))

const out = {
  source: '@dcl/ecs dist-cjs engine component definitions (schema.jsonSchema)',
  ecsVersion,
  generatedAt: new Date().toISOString(),
  registrationErrors: errors,
  componentCount: table.length,
  components: table
}
fs.writeFileSync(outFile, JSON.stringify(out, null, 2) + '\n')
console.log(`wrote ${outFile}: ${table.length} components, ecs ${ecsVersion}, ${errors.length} registration errors`)
for (const e of errors) console.log(`  register-skip: ${e}`)
