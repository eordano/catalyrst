// Headless driver for site/wasm/abgen_poc.wasm — same protocol as
// site/wasm/worker.js, run under node for CI-style verification.
// usage: node headless.mjs <out_dir> <platform> <entity|''> [--lod=0|1] <file...>
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { basename, join } from 'node:path';

let lod = 1;
const positional = [];
for (const a of process.argv.slice(2)) {
  if (a.startsWith('--lod=')) lod = a.slice(6) === '0' ? 0 : 1;
  else positional.push(a);
}
const [outDir, platform, entityType, ...paths] = positional;
if (!outDir || !platform || paths.length === 0) {
  console.error('usage: node headless.mjs <out_dir> <platform> <entity|""> [--lod=0|1] <file...>');
  process.exit(2);
}
mkdirSync(outDir, { recursive: true });

const td = new TextDecoder();
let exports;
let exitOnFatal = 1;

function hostEmit(kind, ptr, len) {
  const bytes = new Uint8Array(exports.memory.buffer, ptr, len).slice();
  if (kind === 0) {
    console.log('event', td.decode(bytes));
  } else if (kind === 1) {
    const dv = new DataView(bytes.buffer);
    const nl = dv.getUint32(0, true);
    const name = td.decode(bytes.subarray(4, 4 + nl));
    const dl = dv.getUint32(4 + nl, true);
    const data = bytes.slice(8 + nl, 8 + nl + dl);
    const safe = name.replace(/[/\\]/g, '__');
    writeFileSync(join(outDir, safe), data);
    console.log('output', name, dl);
  } else if (kind === 2) {
    console.error('FATAL', td.decode(bytes));
    exitOnFatal = 3;
  } else if (kind === 3) {
    writeFileSync(join(outDir, 'manifest.json'), bytes);
    console.log('manifest', bytes.length);
  }
}

const wasiStubs = {
  proc_exit: (code) => { throw new Error(`wasi proc_exit(${code})`); },
  fd_write: (fd, iovs, iovsLen, nwritten) => {
    const dv = new DataView(exports.memory.buffer);
    let total = 0, text = '';
    for (let i = 0; i < iovsLen; i++) {
      const p = dv.getUint32(iovs + i * 8, true);
      const l = dv.getUint32(iovs + i * 8 + 4, true);
      text += td.decode(new Uint8Array(exports.memory.buffer, p, l));
      total += l;
    }
    if (text.trim()) console.log('[wasi]', text.trim());
    dv.setUint32(nwritten, total, true);
    return 0;
  },
  fd_close: () => 0,
  fd_seek: () => 8,
  fd_fdstat_get: () => 8,
  fd_prestat_get: () => 8,
  fd_prestat_dir_name: () => 8,
  environ_sizes_get: (countPtr, sizePtr) => {
    const dv = new DataView(exports.memory.buffer);
    dv.setUint32(countPtr, 0, true);
    dv.setUint32(sizePtr, 0, true);
    return 0;
  },
  environ_get: () => 0,
  clock_time_get: (id, precision, outPtr) => {
    new DataView(exports.memory.buffer)
      .setBigUint64(outPtr, BigInt(Math.round(performance.now() * 1e6)), true);
    return 0;
  },
  random_get: (ptr, len) => {
    crypto.getRandomValues(new Uint8Array(exports.memory.buffer, ptr, len));
    return 0;
  },
};
const wasi = new Proxy(wasiStubs, {
  get: (t, k) => k in t ? t[k] : ((...a) => { console.warn('wasi stub miss:', k); return 52; }),
});

const te = new TextEncoder();
const parts = [];
const u32 = (n) => {
  const b = new Uint8Array(4);
  new DataView(b.buffer).setUint32(0, n, true);
  return b;
};
parts.push(u32(paths.length));
for (const p of paths) {
  const nb = te.encode(basename(p));
  const data = readFileSync(p);
  parts.push(u32(nb.length), nb, u32(data.length), data);
}
const pb = te.encode(platform);
const eb = te.encode(entityType || '');
parts.push(u32(pb.length), pb, u32(eb.length), eb, new Uint8Array([1, lod]));
let total = 0;
for (const p of parts) total += p.byteLength;
const blob = new Uint8Array(total);
let off = 0;
for (const p of parts) { blob.set(p, off); off += p.byteLength; }

const wasmBytes = readFileSync(new URL('../../site/wasm/abgen_poc.wasm', import.meta.url));
const { instance } = await WebAssembly.instantiate(wasmBytes, {
  env: { host_emit: hostEmit },
  wasi_snapshot_preview1: wasi,
});
exports = instance.exports;
exports.poc_init();
const ptr = exports.poc_alloc(blob.length);
new Uint8Array(exports.memory.buffer, ptr, blob.length).set(blob);
const code = exports.poc_convert(ptr, blob.length);
console.log('exit', code);
process.exit(code === 0 ? 0 : exitOnFatal);
