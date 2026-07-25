#!/usr/bin/env node

import fs from 'node:fs'
import path from 'node:path'

const root = path.resolve(process.argv[2] ?? '.')
const sceneJson = JSON.parse(fs.readFileSync(path.join(root, 'scene.json'), 'utf8'))
const mainFile = sceneJson.main
const chunkDir = mainFile.includes('/') ? mainFile.slice(0, mainFile.lastIndexOf('/')) : ''
const sdkChunkRel = chunkDir ? `${chunkDir}/sdk-runtime.js` : 'sdk-runtime.js'
const sceneChunkRel = chunkDir ? `${chunkDir}/scene.js` : 'scene.js'

const consoleLines = []
const hostCalls = { crdtSendToRenderer: 0, crdtGetState: 0, sendBatch: 0, readFile: [] }
const requiredHostModules = new Set()
const backgroundRejections = []
process.on('unhandledRejection', (err) => {
  backgroundRejections.push(String(err && err.stack ? err.stack : err))
})

const sandboxConsole = {}
for (const level of ['log', 'info', 'warn', 'error', 'debug', 'trace']) {
  sandboxConsole[level] = (...args) => {
    consoleLines.push(`[${level}] ${args.map(String).join(' ')}`)
  }
}

function hostModule(name) {
  if (name === '~system/Runtime') {
    return {
      readFile: async ({ fileName }) => {
        hostCalls.readFile.push(fileName)
        const bytes = fs.readFileSync(path.join(root, fileName))
        return { content: new Uint8Array(bytes), hash: 'b64-harness' }
      },
      getRealm: async () => ({
        realmInfo: {
          baseUrl: 'http://127.0.0.1:8000',
          realmName: 'harness',
          networkId: 0,
          commsAdapter: 'offline',
          isPreview: true,
        },
      }),
      getWorldTime: async () => ({ seconds: 0 }),
      getSceneInformation: async () => ({
        urn: 'urn:harness',
        content: [],
        metadataJson: JSON.stringify(sceneJson),
        baseUrl: 'http://127.0.0.1:8000/content/contents/',
      }),
    }
  }
  if (name === '~system/EngineApi') {
    return {
      crdtSendToRenderer: async () => {
        hostCalls.crdtSendToRenderer += 1
        return { data: [] }
      },
      crdtGetState: async () => {
        hostCalls.crdtGetState += 1
        return { data: [], hasEntities: false }
      },
      sendBatch: async () => {
        hostCalls.sendBatch += 1
        return { events: [] }
      },
      subscribe: async () => ({}),
      unsubscribe: async () => ({}),
    }
  }
  if (name === '~system/CommunicationsController') {
    return {
      send: async () => ({ data: [] }),
      sendBinary: async () => ({ data: [] }),
    }
  }
  if (name === '~system/UserIdentity') {
    return {
      getUserData: async () => ({
        data: {
          userId: '0xharness',
          displayName: 'harness',
          hasConnectedWeb3: false,
          version: 1,
          avatar: {
            bodyShape: 'urn:decentraland:off-chain:base-avatars:BaseMale',
            wearables: [],
            emotes: [],
            snapshots: { face256: '', body: '' },
            eyeColor: '#000000',
            hairColor: '#000000',
            skinColor: '#000000',
          },
        },
      }),
      getUserPublicKey: async () => ({ address: '0xharness' }),
    }
  }
  if (name === '~system/Players') {
    return {
      getConnectedPlayers: async () => ({ players: [] }),
      getPlayersInScene: async () => ({ players: [] }),
      getPlayerData: async () => ({}),
    }
  }
  return new Proxy(
    {},
    { get: (_t, prop) => (typeof prop === 'string' ? async () => ({}) : undefined) }
  )
}

function hostRequire(spec) {
  if (typeof spec !== 'string' || !spec.startsWith('~system/')) {
    throw new Error('invalid module request ' + spec)
  }
  requiredHostModules.add(spec)
  return hostModule(spec)
}

const fakeGlobal = {}
fakeGlobal.require = hostRequire
fakeGlobal.console = sandboxConsole

const jsPreamble = [
  'const require = globalThis.require;',
  'const console = globalThis.console;',
  'const TextDecoder = undefined;',
  '',
].join('\n')

function loadCjs(rel) {
  const code = fs.readFileSync(path.join(root, rel), 'utf8')
  const mod = { exports: {} }
  const wrapper = new Function('globalThis', 'module', 'exports', jsPreamble + code)
  wrapper.call(fakeGlobal, fakeGlobal, mod, mod.exports)
  return mod.exports
}

function fail(msg) {
  console.error('HARNESS FAIL: ' + msg)
  if (consoleLines.length) console.error('scene console:\n  ' + consoleLines.join('\n  '))
  process.exit(1)
}

const sizes = {}
for (const rel of [mainFile, sdkChunkRel, sceneChunkRel]) {
  sizes[rel] = fs.statSync(path.join(root, rel)).size
}

const loader = loadCjs(mainFile)
if (typeof loader.onStart !== 'function') fail('loader exports no onStart')
if (typeof loader.onUpdate !== 'function') fail('loader exports no onUpdate')

const early = loader.onUpdate(0.016)
if (early !== undefined) fail('onUpdate before onStart returned ' + early)

await loader.onStart()
if (hostCalls.readFile.length !== 2) fail(`expected 2 readFile calls, got ${hostCalls.readFile.length} (${hostCalls.readFile})`)
if (hostCalls.readFile[0] !== sdkChunkRel || hostCalls.readFile[1] !== sceneChunkRel) {
  fail('readFile order/paths wrong: ' + JSON.stringify(hostCalls.readFile))
}
if (hostCalls.crdtGetState < 1) fail('sdk onStart never called crdtGetState')

for (let frame = 0; frame < 3; frame++) {
  await loader.onUpdate(0.016)
}
if (hostCalls.crdtSendToRenderer < 1) fail('engine never flushed to crdtSendToRenderer during updates')

const marker = 'DCL_ONE_SPLIT_SCENE_MAIN_RAN'
const mainRan = consoleLines.some((l) => l.includes(marker))
if (!mainRan) fail(`scene main() marker "${marker}" not seen in sandbox console`)

const preHostRequires = requiredHostModules.size
const regMod = { exports: {} }
new Function('globalThis', 'module', 'exports', jsPreamble + fs.readFileSync(path.join(root, sdkChunkRel), 'utf8')).call(
  fakeGlobal,
  fakeGlobal,
  regMod,
  regMod.exports
)
if (requiredHostModules.size !== preHostRequires) fail('sdk chunk eval performed ~system requires (registry not lazy)')
const registry = regMod.exports
const keys = Object.keys(registry)
if (keys.length < 20) fail(`registry too small: ${keys.length} keys`)
for (const k of ['@dcl/sdk', '@dcl/ecs', '@dcl/sdk/react-ecs', '~sdk/all-composites', '~sdk/script-utils']) {
  if (!keys.includes(k)) fail(`registry missing key ${k}`)
}
if (registry['@dcl/ecs'] !== registry['@dcl/ecs']) fail('registry values not memoized (identity differs)')

console.log(
  JSON.stringify(
    {
      ok: true,
      sizes,
      registryKeys: keys.length,
      hostCalls: {
        crdtGetState: hostCalls.crdtGetState,
        crdtSendToRenderer: hostCalls.crdtSendToRenderer,
        sendBatch: hostCalls.sendBatch,
        readFile: hostCalls.readFile,
      },
      hostModulesRequired: [...requiredHostModules].sort(),
      sceneMainRan: mainRan,
      sceneConsole: consoleLines,
      backgroundRejections,
    },
    null,
    2
  )
)
