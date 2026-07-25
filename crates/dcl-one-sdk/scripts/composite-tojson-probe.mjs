#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'

const ECS_DEFAULT = 'node_modules/@dcl/ecs'
const ecsRoot = process.env.DCL_ECS_PATH || ECS_DEFAULT
const require = createRequire(import.meta.url)
const ecs = require(path.join(ecsRoot, 'dist-cjs'))
const { Composite, Engine, EntityMappingMode } = ecs

const args = process.argv.slice(2)
if (args.length < 1) {
  console.error('usage: composite-tojson-probe.mjs <out-dir> [--workspace <dir>] [file.composite ...]')
  process.exit(2)
}
const outDir = args[0]
fs.mkdirSync(outDir, { recursive: true })
let workspace = null
const files = []
for (let i = 1; i < args.length; i++) {
  if (args[i] === '--workspace') workspace = args[++i]
  else files.push(args[i])
}

const report = []
const log = (...a) => {
  const line = a.join(' ')
  report.push(line)
  console.log(line)
}

function normalizePure(rawJson) {
  return Composite.toJson(Composite.fromJson(rawJson))
}

function instanceOnFreshEngine(src, composite) {
  const engine = Engine()
  const provider = {
    getCompositeOrNull(reqSrc) {
      if (reqSrc === src) return { src, composite }
      return null
    }
  }
  Composite.instance(engine, { src, composite }, provider, {
    entityMapping: {
      type: EntityMappingMode.EMM_DIRECT_MAPPING,
      getCompositeEntity: (e) => e
    }
  })
}

for (const file of files) {
  const base = path.basename(file).replace(/\.composite$/, '')
  const rawText = fs.readFileSync(file, 'utf8')
  const rawJson = JSON.parse(rawText)
  const pure = normalizePure(rawJson)
  const pureStr = JSON.stringify(pure)
  fs.writeFileSync(path.join(outDir, `${base}.raw.json`), rawText)
  fs.writeFileSync(path.join(outDir, `${base}.tojson.json`), pureStr + '\n')
  fs.writeFileSync(path.join(outDir, `${base}.raw.pretty.json`), JSON.stringify(rawJson, null, 2) + '\n')
  fs.writeFileSync(path.join(outDir, `${base}.tojson.pretty.json`), JSON.stringify(pure, null, 2) + '\n')

  const parsed = Composite.fromJson(rawJson)
  const before = JSON.stringify(Composite.toJson(parsed))
  let leak = 'not-instanced'
  try {
    instanceOnFreshEngine(file, parsed)
    const after = JSON.stringify(Composite.toJson(parsed))
    leak = before === after ? 'no-mutation' : 'MUTATION-LEAK'
    if (leak === 'MUTATION-LEAK') {
      fs.writeFileSync(path.join(outDir, `${base}.after-instance.json`), after + '\n')
    }
  } catch (err) {
    leak = `instance-failed: ${err.message}`
  }
  log(`file=${file} bytes_raw=${rawText.length} bytes_tojson=${pureStr.length} idempotent=${JSON.stringify(normalizePure(pure)) === pureStr} instance_check=${leak}`)
}

if (workspace) {
  const composites = {}
  const found = []
  const walk = (dir) => {
    for (const name of fs.readdirSync(dir).sort()) {
      const p = path.join(dir, name)
      const st = fs.statSync(p)
      if (st.isDirectory()) {
        if (name !== 'node_modules' && !name.startsWith('.')) walk(p)
      } else if (name.endsWith('.composite')) {
        found.push(path.relative(workspace, p))
      }
    }
  }
  walk(workspace)
  for (const rel of found) {
    composites[rel] = Composite.fromJson(JSON.parse(fs.readFileSync(path.join(workspace, rel), 'utf8')))
  }
  const provider = {
    getCompositeOrNull(src) {
      if (src in composites) return { src, composite: composites[src] }
      return null
    }
  }
  const engine = Engine()
  const compositeLines = []
  for (const src in composites) {
    try {
      const resource = provider.getCompositeOrNull(src)
      Composite.instance(engine, resource, provider, {
        entityMapping: {
          type: EntityMappingMode.EMM_DIRECT_MAPPING,
          getCompositeEntity: (e) => e
        }
      })
      compositeLines.push(`'${resource.src}':${JSON.stringify(Composite.toJson(resource.composite))}`)
    } catch (err) {
      log(`workspace-instance-error src=${src} err=${err.message}`)
    }
  }
  const moduleText = `export const compositeFromLoader = {${compositeLines.join(',')}}`
  fs.writeFileSync(path.join(outDir, 'all-composites.expected.js'), moduleText + '\n')
  log(`workspace=${workspace} files=${found.length} emitted=${compositeLines.length} -> all-composites.expected.js`)
}

const edgeCases = [
  ['empty object', {}],
  ['string version', { version: '1' }],
  ['float version rounds', { version: 1.6 }],
  ['unknown fields dropped', { version: 1, extra: 'x', components: [{ name: 'foo', unknown: 1, data: {} }] }],
  ['empty component defaults', { components: [{}] }],
  ['numeric name coerced', { components: [{ name: 42 }] }],
  ['data keys sorted ascending', { version: 1, components: [{ name: 'a', data: { 513: { json: 1 }, 512: { json: 2 } } }] }],
  ['non-canonical key collides', { version: 1, components: [{ name: 'a', data: { '512': { json: 'canonical' }, '0512': { json: 'padded' } } }] }],
  ['$case dropped', { version: 1, components: [{ name: 'a', data: { 512: { $case: 'json', json: { x: 1 } } } }] }],
  ['json null becomes empty', { version: 1, components: [{ name: 'a', data: { 512: { json: null } } }] }],
  ['json falsy preserved', { version: 1, components: [{ name: 'a', data: { 512: { json: false }, 513: { json: 0 }, 514: { json: '' } } }] }],
  ['payload key order verbatim', { version: 1, components: [{ name: 'a', data: { 512: { json: { b: 1, a: 2 } } } }] }],
  ['json wins over binary', { version: 1, components: [{ name: 'a', data: { 512: { json: { x: 1 }, binary: 'AAAA' } } }] }],
  ['binary repadded', { version: 1, components: [{ name: 'a', data: { 512: { binary: 'AAA' } } }] }],
  ['binary urlsafe recanonicalized', { version: 1, components: [{ name: 'a', data: { 512: { binary: '-_x8' } } }] }],
  ['binary empty', { version: 1, components: [{ name: 'a', data: { 512: { binary: '' } } }] }],
  ['jsonSchema verbatim', { version: 1, components: [{ name: 'a', jsonSchema: { whatever: true, zz: 1, aa: 2 }, data: {} }] }],
  ['jsonSchema absent stays absent', { version: 1, components: [{ name: 'core::Transform', data: { 512: { json: { position: { x: 1, y: 1, z: 1 } } } } }] }],
  ['above-max-index key order', { version: 1, components: [{ name: 'a', data: { 4294967295: { json: 1 }, 42: { json: 2 } } }] }],
  ['negative key', { version: 1, components: [{ name: 'a', data: { '-1': { json: 1 }, 5: { json: 2 } } }] }],
  ['float key', { version: 1, components: [{ name: 'a', data: { 1.5: { json: 1 } } }] }],
  ['non-numeric key becomes NaN', { version: 1, components: [{ name: 'a', data: { abc: { json: 1 } } }] }],
  ['data null becomes empty', { version: 1, components: [{ name: 'a', data: null }] }],
  ['components null becomes empty', { version: 1, components: null }]
]

const edgeResults = []
for (const [label, input] of edgeCases) {
  let output
  try {
    output = JSON.stringify(normalizePure(input))
  } catch (err) {
    output = `THROWS: ${err.message}`
  }
  edgeResults.push({ label, input, output })
  log(`edge: ${label}\n  in:  ${JSON.stringify(input)}\n  out: ${output}`)
}
fs.writeFileSync(path.join(outDir, 'edge-cases.json'), JSON.stringify(edgeResults, null, 2) + '\n')
fs.writeFileSync(path.join(outDir, 'probe-log.txt'), report.join('\n') + '\n')
log(`done out=${outDir}`)
