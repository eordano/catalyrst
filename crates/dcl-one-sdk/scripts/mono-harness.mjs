#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'

const bundle = path.resolve(process.argv[2])
const root = path.resolve(process.argv[3] ?? '.')
const sceneJson = JSON.parse(fs.readFileSync(path.join(root, 'scene.json'), 'utf8'))

const consoleLines = []
const hostCalls = { crdtSendToRenderer: 0, crdtGetState: 0, sendBatch: 0, readFile: [] }
const requiredHostModules = new Set()
const backgroundRejections = []
process.on('unhandledRejection', (err) => {
  backgroundRejections.push(String(err && err.stack ? err.stack : err))
})

const sandboxConsole = {}
for (const level of ['log', 'info', 'warn', 'error', 'debug', 'trace']) {
  sandboxConsole[level] = (...args) => consoleLines.push(`[${level}] ${args.map(String).join(' ')}`)
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
        realmInfo: { baseUrl: 'http://127.0.0.1:8000', realmName: 'harness', networkId: 0, commsAdapter: 'offline', isPreview: true },
      }),
      getWorldTime: async () => ({ seconds: 0 }),
      getSceneInformation: async () => ({
        urn: 'urn:harness', content: [], metadataJson: JSON.stringify(sceneJson),
        baseUrl: 'http://127.0.0.1:8000/content/contents/',
      }),
    }
  }
  if (name === '~system/EngineApi') {
    return {
      crdtSendToRenderer: async () => { hostCalls.crdtSendToRenderer += 1; return { data: [] } },
      crdtGetState: async () => { hostCalls.crdtGetState += 1; return { data: [], hasEntities: false } },
      sendBatch: async () => { hostCalls.sendBatch += 1; return { events: [] } },
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
          userId: '0xharness', displayName: 'harness', hasConnectedWeb3: false, version: 1,
          avatar: {
            bodyShape: 'urn:decentraland:off-chain:base-avatars:BaseMale',
            wearables: [], emotes: [], snapshots: { face256: '', body: '' },
            eyeColor: '#000000', hairColor: '#000000', skinColor: '#000000',
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
  return new Proxy({}, { get: (_t, prop) => (typeof prop === 'string' ? async () => ({}) : undefined) })
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

function fail(msg) {
  console.error('HARNESS FAIL: ' + msg)
  if (consoleLines.length) console.error('scene console:\n  ' + consoleLines.join('\n  '))
  process.exit(1)
}

const code = fs.readFileSync(bundle, 'utf8')
const mod = { exports: {} }
const wrapper = new Function('globalThis', 'module', 'exports', jsPreamble + code)
wrapper.call(fakeGlobal, fakeGlobal, mod, mod.exports)
const scene = mod.exports

if (typeof scene.onStart !== 'function') fail('bundle exports no onStart')
if (typeof scene.onUpdate !== 'function') fail('bundle exports no onUpdate')

await scene.onStart()
if (hostCalls.crdtGetState < 1) fail('onStart never called crdtGetState')
for (let frame = 0; frame < 3; frame++) {
  await scene.onUpdate(0.016)
}
if (hostCalls.crdtSendToRenderer < 1) fail('engine never flushed to crdtSendToRenderer')

console.log(JSON.stringify({
  ok: true,
  bundle: path.basename(bundle),
  bytes: fs.statSync(bundle).size,
  exportsCount: Object.keys(scene).length,
  hostCalls: { crdtGetState: hostCalls.crdtGetState, crdtSendToRenderer: hostCalls.crdtSendToRenderer, sendBatch: hostCalls.sendBatch, readFile: hostCalls.readFile },
  hostModulesRequired: [...requiredHostModules].sort(),
  sceneConsole: consoleLines,
  backgroundRejections,
}))
