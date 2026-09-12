'use strict'

const path = require('path')

const shim = process.argv[2]
if (!shim) {
  console.error('usage: check-editor-host-runtime.cjs <shim-dir>')
  process.exit(2)
}

function memoryFs() {
  const files = new Map()
  const toPosix = (p) => p.replace(/\\/g, '/')
  return {
    files,
    dirname: (p) => toPosix(path.dirname(p)),
    basename: (p) => toPosix(path.basename(p)),
    join: (...p) => toPosix(path.join(...p)),
    async existFile(p) {
      return files.has(toPosix(p))
    },
    async readFile(p) {
      const found = files.get(toPosix(p))
      if (!found) throw new Error(`ENOENT: ${p}`)
      return found
    },
    async writeFile(p, content) {
      files.set(toPosix(p), Buffer.from(content))
    },
    async readdir(dirPath) {
      const prefix = dirPath === '' || dirPath === '.' ? '' : toPosix(dirPath) + '/'
      const names = new Set()
      for (const key of files.keys()) {
        if (!key.startsWith(prefix)) continue
        const rest = key.slice(prefix.length)
        const cut = rest.indexOf('/')
        names.add(cut === -1 ? rest : rest.slice(0, cut))
      }
      return [...names].map((name) => ({
        name,
        isDirectory: [...files.keys()].some((k) => k.startsWith(prefix + name + '/'))
      }))
    }
  }
}

function checkRootComponents(engine, failures, label) {
  const snapshot = require(path.join(shim, 'root-components.json')).components
  for (const name of Object.keys(snapshot)) {
    const component = engine.getComponentOrNull(name)
    if (!component) {
      failures.push(`${label}: ${name} is not defined on the engine`)
      continue
    }
    if (!component.has(engine.RootEntity)) {
      failures.push(`${label}: ${name} missing on the root entity after boot`)
    }
  }
  const actionTypes = engine.getComponentOrNull('asset-packs::ActionTypes')
  const list = actionTypes && actionTypes.getOrNull(engine.RootEntity)
  const have = new Set((list ? list.value : []).map(($) => $.type))
  for (const entry of snapshot['asset-packs::ActionTypes'].value) {
    if (!have.has(entry.type)) failures.push(`${label}: action type ${entry.type} not seeded`)
  }
  return list
}

function compositeWithRootState() {
  const minimal = require(path.join(shim, 'minimal-composite.json'))
  const snapshot = require(path.join(shim, 'root-components.json')).components
  const schemaOf = (name) =>
    require(path.join(shim, 'component-schemas.json')).components.find((c) => c.name === name)
      .jsonSchema
  return {
    ...minimal,
    components: [
      ...minimal.components,
      {
        name: 'asset-packs::ActionTypes',
        jsonSchema: schemaOf('asset-packs::ActionTypes'),
        data: {
          0: {
            json: {
              value: [
                { type: 'from_the_future', jsonSchema: '{}' },
                { ...snapshot['asset-packs::ActionTypes'].value[0], jsonSchema: '"older"' }
              ]
            }
          }
        }
      },
      {
        name: 'asset-packs::Counter',
        jsonSchema: schemaOf('asset-packs::Counter'),
        data: { 0: { json: { id: 0, value: 5 } } }
      }
    ]
  }
}

async function main() {
  const { createDataLayerHost } = require(path.join(shim, 'host.js'))
  const { DataServiceDefinition } = require(path.join(shim, 'data-layer.gen.js'))

  const expected = Object.keys(DataServiceDefinition.methods)
  const { rpcMethods, engine } = await createDataLayerHost(memoryFs())

  const failures = []

  checkRootComponents(engine, failures, 'fresh scene')

  const seededFs = memoryFs()
  await seededFs.writeFile(
    'assets/scene/main.composite',
    JSON.stringify(compositeWithRootState())
  )
  const reopened = await createDataLayerHost(seededFs)
  const list = checkRootComponents(reopened.engine, failures, 'existing composite')
  if (list) {
    const byType = new Map(list.value.map(($) => [$.type, $]))
    if (!byType.has('from_the_future')) {
      failures.push('existing composite: an unknown action type was dropped -- seeding overwrote the list')
    }
    const first = require(path.join(shim, 'root-components.json')).components[
      'asset-packs::ActionTypes'
    ].value[0]
    const kept = byType.get(first.type)
    if (!kept || kept.jsonSchema !== first.jsonSchema) {
      failures.push(`existing composite: ${first.type} was not replaced by the snapshot's entry`)
    }
    if (list.value.filter(($) => $.type === first.type).length !== 1) {
      failures.push(`existing composite: ${first.type} appears more than once after seeding`)
    }
  }
  const counter = reopened.engine.getComponentOrNull('asset-packs::Counter')
  const counterValue = counter && counter.getOrNull(reopened.engine.RootEntity)
  if (!counterValue || counterValue.value !== 5) {
    failures.push(
      `existing composite: Counter came back as ${JSON.stringify(counterValue)} -- expected the saved value 5`
    )
  }

  for (const name of expected) {
    if (typeof rpcMethods[name] !== 'function') {
      failures.push(`${name}: missing -- registerService would throw at port setup`)
    }
  }

  const streaming = new Set(['crdtStream'])

  for (const name of expected) {
    if (streaming.has(name) || typeof rpcMethods[name] !== 'function') continue
    let result
    try {
      result = await rpcMethods[name]({})
    } catch (err) {
      continue
    }
    if (result === undefined || result === null) {
      failures.push(`${name}: resolved ${result} -- serverProcedureUnary rejects falsy responses`)
    }
  }

  if (failures.length) {
    console.error('check-editor-host-runtime: FAILED')
    for (const f of failures) console.error('  ' + f)
    process.exit(1)
  }
  const seeded = Object.keys(require(path.join(shim, 'root-components.json')).components)
  console.log(
    `check-editor-host-runtime: OK -- ${expected.length} methods present, ` +
      `${expected.length - streaming.size} return non-empty responses, ` +
      `root seeded with ${seeded.join(', ')} on fresh and existing composites`
  )
}

main().catch((err) => {
  console.error('check-editor-host-runtime: could not boot the host')
  console.error(err)
  process.exit(1)
})
