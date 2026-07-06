// Headless driver for the worker-pool path: site/wasm/pool.js + worker.js run
// under node worker_threads with the same protocol as the browser page.
// usage: node headless-pool.mjs <out_dir> <platform> <entity|''>
//        [--lod=0|1] [--workers=N] <file...>
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { basename, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Worker } from 'node:worker_threads';
import { runConvert } from '../../site/wasm/pool.js';

let lod = 1, workers = 0;
const positional = [];
for (const a of process.argv.slice(2)) {
  if (a.startsWith('--lod=')) lod = a.slice(6) === '0' ? 0 : 1;
  else if (a.startsWith('--workers=')) workers = Number(a.slice(10)) || 0;
  else positional.push(a);
}
const [outDir, platform, entityType, ...paths] = positional;
if (!outDir || !platform || paths.length === 0) {
  console.error('usage: node headless-pool.mjs <out_dir> <platform> <entity|""> ' +
    '[--lod=0|1] [--workers=N] <file...>');
  process.exit(2);
}
mkdirSync(outDir, { recursive: true });

const shim = fileURLToPath(new URL('./worker-shim.mjs', import.meta.url));
const workerJs = fileURLToPath(new URL('../../site/wasm/worker.js', import.meta.url));
const spawn = () => {
  const w = new Worker(shim, { workerData: { workerJs } });
  const like = {
    onmessage: null,
    onerror: null,
    postMessage: (m, t) => w.postMessage(m, t),
    terminate: () => w.terminate(),
  };
  w.on('message', (d) => { if (like.onmessage) like.onmessage({ data: d }); });
  w.on('error', (err) => {
    if (like.onerror) like.onerror({ message: String((err && err.message) || err) });
  });
  return like;
};

const files = paths.map((p) => {
  const buf = readFileSync(p);
  return {
    name: basename(p),
    data: buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength),
  };
});
const wasmBytes = readFileSync(new URL('../../site/wasm/abgen_poc.wasm', import.meta.url));

const t0 = performance.now();
let exit = 0;
await new Promise((resolve) => {
  runConvert({
    files,
    module: WebAssembly.compile(wasmBytes),
    platform,
    entityType: entityType || '',
    magenta: true,
    lod: lod === 1,
    size: workers || undefined,
    spawn,
  }, (m) => {
    const w = m.worker === undefined ? '-' : m.worker;
    if (m.type === 'ready') console.log('ready');
    else if (m.type === 'event') console.log(`event w${w}`, JSON.stringify(m.data));
    else if (m.type === 'output') {
      writeFileSync(join(outDir, m.name.replace(/[/\\]/g, '__')), Buffer.from(m.data));
      console.log(`output w${w}`, m.name, m.size);
    } else if (m.type === 'fatal') console.error(`FATAL w${w}`, m.msg);
    else if (m.type === 'manifest') {
      writeFileSync(join(outDir, 'manifest.json'), JSON.stringify(m.json));
      console.log('manifest', JSON.stringify(m.json));
    } else if (m.type === 'done') {
      const secs = ((performance.now() - t0) / 1000).toFixed(2);
      console.log(`POOL: workers=${m.workers} jobs=${m.jobs} wall=${secs}s exit=${m.code}`);
      exit = m.code;
      resolve();
    }
  });
});
process.exit(exit === 0 ? 0 : 1);
