import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'

const source = readFileSync(new URL('../src/start/page_common.js', import.meta.url), 'utf8')

function page(fetch) {
  const timers = new Map(), listeners = new Map()
  let nextTimer = 0, offline = false
  const status = { dataset: { healthUrl: '/preview/session/about' }, textContent: '', title: '' }
  const bar = { classList: { toggle(name, value) { if (name === 'bar--offline') offline = value } } }
  const navigator = { onLine: true }
  const context = {
    fetch, navigator, AbortController,
    document: {
      querySelector: selector => selector === '.bar' ? bar : null,
      querySelectorAll: () => [],
      getElementById: id => id === 'preview-connection' ? status : null,
    },
    window: { addEventListener: (event, callback) => listeners.set(event, callback) },
    setTimeout: (fn, ms) => { const id = ++nextTimer; timers.set(id, { fn, ms }); return id },
    clearTimeout: id => timers.delete(id),
  }
  vm.runInNewContext(source, context)
  return {
    status, navigator, listeners,
    offline: () => offline,
    async settle() { await new Promise(resolve => setImmediate(resolve)) },
    fire(ms) {
      const entry = [...timers].find(([, timer]) => timer.ms === ms)
      assert.ok(entry, `missing ${ms}ms timer`)
      timers.delete(entry[0]); entry[1].fn()
    },
  }
}

test('header loses and regains preview connectivity, respecting forwarded prefix', async () => {
  let ok = true
  const p = page(async (url, options) => {
    assert.equal(url, '/preview/session/about')
    assert.equal(options.cache, 'no-store')
    if (!ok) throw new Error('connection refused')
    return { ok: true, json: async () => ({ acceptingUsers: true }) }
  })
  await p.settle()
  assert.equal(p.offline(), false)
  ok = false; p.fire(3000); await p.settle()
  assert.equal(p.offline(), true)
  assert.match(p.status.textContent, /disconnected/)
  ok = true; p.fire(3000); await p.settle()
  assert.equal(p.offline(), false)
  assert.equal(p.status.textContent, '')
  p.listeners.get('offline')()
  assert.equal(p.offline(), true)
})

test('header detects hanging requests and resumes polling', async () => {
  const p = page((_url, { signal }) => new Promise((_, reject) => {
    signal.addEventListener('abort', () => reject(new Error('aborted')))
  }))
  p.fire(4000); await p.settle()
  assert.equal(p.offline(), true)
  p.fire(3000)
  p.fire(4000); await p.settle()
  assert.equal(p.offline(), true)
})

test('HTTP errors and unexpected health responses do not report connected', async () => {
  for (const response of [{ ok: false }, { ok: true, json: async () => ({}) }]) {
    const p = page(async () => response)
    await p.settle()
    assert.equal(p.offline(), true)
  }
})
