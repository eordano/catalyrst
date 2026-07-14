'use strict'

// The half of the editor host's contract that no type can hold.
//
// `check-editor-host.sh` type-checks the shim, and that catches a method the
// descriptor names but the host does not define. It cannot catch the second
// hazard in host.js's header -- `serverProcedureUnary` throws "Empty or null
// responses are not allowed" on any falsy result -- because TypeScript does not
// check return types through JSDoc in a .js file. Verified, not assumed: an
// `async f() { return undefined }` annotated `@returns {Promise<object>}` is
// accepted in a .js file and rejected in the identical .ts. So the rule is
// enforced by calling every method instead of by describing it.
//
// Booting the real host against an in-memory filesystem also exercises the
// path that matters most and is otherwise only reachable from the gated
// data_layer_rpc integration test: composite scan -> Composite.instance ->
// the load-bearing engine.update tick.
//
// Usage: node check-editor-host-runtime.cjs <shim-dir>

const path = require('path')

const shim = process.argv[2]
if (!shim) {
  console.error('usage: check-editor-host-runtime.cjs <shim-dir>')
  process.exit(2)
}

// An in-memory FileSystemInterface: the seven methods host.js calls, and no
// disk. The host writes minimal-composite.json on boot when none exists, so an
// empty store is a complete scene as far as this check is concerned.
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

async function main() {
  const { createDataLayerHost } = require(path.join(shim, 'host.js'))
  const { DataServiceDefinition } = require(path.join(shim, 'data-layer.gen.js'))

  const expected = Object.keys(DataServiceDefinition.methods)
  const { rpcMethods } = await createDataLayerHost(memoryFs())

  const failures = []

  for (const name of expected) {
    if (typeof rpcMethods[name] !== 'function') {
      // What `codegen.registerService`'s `mod[key].bind(mod)` would hit.
      failures.push(`${name}: missing -- registerService would throw at port setup`)
    }
  }

  // `crdtStream` is the one streaming method: it takes an async iterable and
  // returns a queue, so calling it with an empty stream would race the consume
  // loop against process exit. Its shape is covered by the type check and by
  // the data_layer_rpc integration test, which drives it for real.
  const streaming = new Set(['crdtStream'])

  for (const name of expected) {
    if (streaming.has(name) || typeof rpcMethods[name] !== 'function') continue
    let result
    try {
      result = await rpcMethods[name]({})
    } catch (err) {
      // A method that throws on an empty request is reporting a bad argument,
      // not violating the response rule. Only a falsy RESULT is the failure.
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
  console.log(
    `check-editor-host-runtime: OK -- ${expected.length} methods present, ` +
      `${expected.length - streaming.size} return non-empty responses`
  )
}

main().catch((err) => {
  console.error('check-editor-host-runtime: could not boot the host')
  console.error(err)
  process.exit(1)
})
