// node worker_threads bootstrap that gives site/wasm/worker.js the browser
// worker globals it expects (self/postMessage/onmessage). onmessage must
// pre-exist on globalThis or the worker's strict-mode assignment throws.
import { parentPort, workerData } from 'node:worker_threads';
import { pathToFileURL } from 'node:url';

globalThis.self = globalThis;
globalThis.onmessage = null;
globalThis.postMessage = (msg, transfer) => parentPort.postMessage(msg, transfer);
parentPort.on('message', (data) => {
  const h = globalThis.onmessage;
  if (h) h({ data });
});
await import(pathToFileURL(workerData.workerJs).href);
