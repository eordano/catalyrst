import { createRequire } from 'node:module'
import { createHash } from 'node:crypto'
import path from 'node:path'
import fs from 'node:fs'

const sceneDir = path.resolve(process.argv[2])
const previewPort = process.argv[3]
const cdpPort = process.argv[4]
const evidenceDir = path.resolve(process.argv[5])

const req = createRequire(path.join(sceneDir, 'package.json'))
const WebSocket = req('ws')

fs.mkdirSync(evidenceDir, { recursive: true })

const composite = path.join(sceneDir, 'assets/scene/main.composite')
const crdt = path.join(sceneDir, 'main.crdt')
const entityName = 'TourCube'
const xValue = '7.25'

function fail(msg) {
  console.error('FAIL:', msg)
  process.exit(1)
}

async function retry(what, timeoutMs, fn) {
  const deadline = Date.now() + timeoutMs
  let last
  while (Date.now() < deadline) {
    try {
      const v = await fn()
      if (v) return v
    } catch (e) {
      last = e
    }
    await new Promise((r) => setTimeout(r, 500))
  }
  fail(`timed out waiting for ${what}${last ? `: ${last}` : ''}`)
}

const sceneUpdates = []
const updatesWs = new WebSocket(`ws://127.0.0.1:${previewPort}/`)
updatesWs.on('message', (data, isBinary) => {
  if (isBinary) return
  try {
    const v = JSON.parse(data.toString())
    if (v.type === 'SCENE_UPDATE') sceneUpdates.push(Date.now())
  } catch {}
})

const tab = await (
  await fetch(`http://127.0.0.1:${cdpPort}/json/new?about:blank`, { method: 'PUT' })
).json()
const cdp = new WebSocket(tab.webSocketDebuggerUrl, { maxPayload: 256 * 1024 * 1024 })
let idc = 0
const pending = new Map()
const wsCreated = []
cdp.on('message', (data) => {
  const m = JSON.parse(data.toString())
  if (m.id && pending.has(m.id)) {
    const { resolve, reject } = pending.get(m.id)
    pending.delete(m.id)
    if (m.error) reject(new Error(JSON.stringify(m.error)))
    else resolve(m.result)
  } else if (m.method === 'Network.webSocketCreated') {
    wsCreated.push(m.params.url)
  }
})
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++idc
    pending.set(id, { resolve, reject })
    cdp.send(JSON.stringify({ id, method, params }))
  })
await new Promise((r) => cdp.on('open', r))
await send('Page.enable')
await send('Runtime.enable')
await send('Network.enable')

const evalJson = async (expression) => {
  const r = await send('Runtime.evaluate', {
    expression: `(() => { try { return JSON.stringify(${expression}) } catch (e) { return JSON.stringify({ __err: String(e) }) } })()`,
    returnByValue: true
  })
  return JSON.parse(r.result.value)
}
const click = async (x, y, button = 'left') => {
  await send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button, clickCount: 1 })
  await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button, clickCount: 1 })
}
const pressEnter = async () => {
  await send('Input.dispatchKeyEvent', {
    type: 'keyDown',
    key: 'Enter',
    code: 'Enter',
    windowsVirtualKeyCode: 13
  })
  await send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    key: 'Enter',
    code: 'Enter',
    windowsVirtualKeyCode: 13
  })
}

await send('Page.navigate', { url: `http://127.0.0.1:${previewPort}/inspector/` })
console.log('step: navigated to /inspector/')

await retry('the injected InspectorConfig', 30000, () =>
  evalJson('globalThis.InspectorConfig && globalThis.InspectorConfig.dataLayerRpcWsUrl')
)
console.log('step: InspectorConfig injected')

await retry('the /data-layer websocket', 30000, async () =>
  wsCreated.some((u) => u.includes('/data-layer'))
)
console.log('step: browser opened the /data-layer websocket')

await retry('the hierarchy tree', 60000, async () => {
  const items = await evalJson(
    "Array.from(document.querySelectorAll('.Tree .item-area')).map(e => (e.textContent||'').trim())"
  )
  return Array.isArray(items) && items.includes('Scene')
})
console.log('step: hierarchy rendered')

const compositeBefore = fs.existsSync(composite) ? fs.readFileSync(composite, 'utf8') : ''
if (compositeBefore.includes(entityName)) fail('fixture already contains the test entity')
const editStarted = Date.now()

const sceneBox = await evalJson(
  "(function(){ const it = Array.from(document.querySelectorAll('.Tree .item-area')).find(e => (e.textContent||'').trim() === 'Scene'); const b = it.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 } })()"
)
await click(sceneBox.x, sceneBox.y, 'right')
const addChild = await retry('the Add child menu item', 15000, () =>
  evalJson(
    "(function(){ const it = Array.from(document.querySelectorAll('.contexify_itemContent')).find(e => (e.textContent||'').includes('Add child')); if (!it) return null; const b = it.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 } })()"
  )
)
await click(addChild.x, addChild.y)
await new Promise((r) => setTimeout(r, 800))
await send('Input.insertText', { text: entityName })
await pressEnter()
await retry('the new entity in the hierarchy', 15000, async () => {
  const items = await evalJson(
    "Array.from(document.querySelectorAll('.Tree .item-area')).map(e => (e.textContent||'').trim())"
  )
  return items.includes(entityName)
})
console.log('step: entity added via the hierarchy UI')

const entityBox = await evalJson(
  `(function(){ const it = Array.from(document.querySelectorAll('.Tree .item-area')).find(e => (e.textContent||'').trim() === '${entityName}'); const b = it.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 } })()`
)
await click(entityBox.x, entityBox.y)
const xInput = await retry('the Transform position inputs', 15000, () =>
  evalJson(
    "(function(){ const ei = document.querySelector('.EntityInspector'); if (!ei || !(ei.textContent||'').includes('Transform')) return null; const inp = ei.querySelectorAll('input')[0]; if (!inp) return null; const b = inp.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 } })()"
  )
)
await click(xInput.x, xInput.y)
await new Promise((r) => setTimeout(r, 300))
await send('Input.dispatchKeyEvent', {
  type: 'keyDown',
  key: 'a',
  code: 'KeyA',
  modifiers: 2,
  windowsVirtualKeyCode: 65
})
await send('Input.dispatchKeyEvent', {
  type: 'keyUp',
  key: 'a',
  code: 'KeyA',
  modifiers: 2,
  windowsVirtualKeyCode: 65
})
await send('Input.insertText', { text: xValue })
await pressEnter()
console.log('step: transform X set through the panel')

await retry('the autosaved composite on disk', 60000, async () => {
  if (!fs.existsSync(composite)) return false
  const now = fs.readFileSync(composite, 'utf8')
  return now.includes(entityName) && now.includes(`"x": ${xValue}`)
})
console.log('step: assets/scene/main.composite saved with the edit')

await retry('the regenerated main.crdt', 60000, async () => {
  if (!fs.existsSync(crdt)) return false
  return fs.statSync(crdt).mtimeMs >= fs.statSync(composite).mtimeMs - 1
})
console.log('step: main.crdt regenerated after the composite save')

await retry('a SCENE_UPDATE push', 60000, async () =>
  sceneUpdates.some((t) => t >= editStarted)
)
console.log('step: SCENE_UPDATE received on the reload channel')

const shot = await send('Page.captureScreenshot', { format: 'png' })
fs.writeFileSync(path.join(evidenceDir, 'inspector-ui.png'), Buffer.from(shot.data, 'base64'))

const sha = (p) => createHash('sha256').update(fs.readFileSync(p)).digest('hex')
const summary = {
  ok: true,
  composite: { path: composite, sha256: sha(composite), bytes: fs.statSync(composite).size },
  crdt: { path: crdt, sha256: sha(crdt), bytes: fs.statSync(crdt).size },
  sceneUpdatesAfterEdit: sceneUpdates.filter((t) => t >= editStarted).length,
  dataLayerWs: wsCreated.filter((u) => u.includes('/data-layer'))
}
fs.writeFileSync(path.join(evidenceDir, 'ui-drive-summary.json'), JSON.stringify(summary, null, 2))
console.log('RESULT', JSON.stringify(summary))
process.exit(0)
