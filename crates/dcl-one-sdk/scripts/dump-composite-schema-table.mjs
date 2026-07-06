#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'

const ECS_DEFAULT = 'node_modules/@dcl/ecs'
const ecsRoot = process.env.DCL_ECS_PATH || ECS_DEFAULT
const outFile = process.argv[2]
if (!outFile) {
  console.error('usage: dump-composite-schema-table.mjs <out-file.json>')
  process.exit(2)
}
const require = createRequire(import.meta.url)
const ecs = require(path.join(ecsRoot, 'dist-cjs'))
const gen = require(path.join(ecsRoot, 'dist-cjs/components/generated/index.gen.js'))
const comps = require(path.join(ecsRoot, 'dist-cjs/components'))
const compositeComponents = require(path.join(ecsRoot, 'dist-cjs/composite/components'))
const ecsVersion = require(path.join(ecsRoot, 'package.json')).version

const engine = ecs.Engine()
compositeComponents.getCompositeRootComponent(engine)
const errors = []
for (const [name, factory] of Object.entries(gen.componentDefinitionByName)) {
  try {
    factory(engine)
  } catch (err) {
    errors.push(`${name}: ${err.message}`)
  }
}
for (const [name, factory] of Object.entries(comps)) {
  if (typeof factory !== 'function') continue
  try {
    factory(engine)
  } catch (err) {
    errors.push(`components.${name}: ${err.message}`)
  }
}

const table = []
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
