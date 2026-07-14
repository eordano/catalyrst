import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { freePort, launchChromium, Tab } from "./cdp.mts";

const args = process.argv.slice(2);
function opt(name: string, fallback?: string): string | undefined {
  const i = args.indexOf(`--${name}`);
  return i === -1 ? fallback : args[i + 1];
}

const url = opt("url");
const out = opt("out");
const settleMs = Number(opt("settle", "9000"));
const endpoint = opt("endpoint", "http://127.0.0.1:5199");
const profileDir = opt("profile", path.join(os.tmpdir(), "telem-capture-profile"))!;
if (!url || !out) {
  console.error(
    "usage: node scripts/drive/telemetry-capture.mts --url <url> --out <file> [--settle ms] [--profile dir]",
  );
  process.exit(2);
}

type TrackBody = { url: string; postData: string | undefined };

fs.mkdirSync(profileDir, { recursive: true });
const port = await freePort();
const proc = await launchChromium({ port, profileDir });
let bodies: TrackBody[] = [];
try {
  const tab = await Tab.open(port);
  await tab.cmd("Network.enable");
  await tab.cmd("Page.addScriptToEvaluateOnNewDocument", {
    source: `globalThis.process = { env: { TELEMETRY_URL: ${JSON.stringify(endpoint)} } };`,
  });
  await tab.setViewport(1600, 1000);
  await tab.navigate(url, settleMs);
  const pending = new Map<string, TrackBody>();
  for (const ev of tab.drainEvents()) {
    if (ev.method !== "Network.requestWillBeSent") continue;
    const req = ev.params?.request;
    if (req.method !== "POST") continue;
    if (!/\/v1\/track$/.test(req.url.split("?")[0])) continue;
    pending.set(ev.params!.requestId, { url: req.url, postData: req.postData });
  }
  for (const [requestId, entry] of pending) {
    if (entry.postData !== undefined) continue;
    try {
      const r = await tab.cmd("Network.getRequestPostData", { requestId });
      entry.postData = r.postData;
    } catch {
    }
  }
  bodies = [...pending.values()];
} finally {
  proc.kill();
}

bodies.sort((a, b) => (a.postData ?? "").localeCompare(b.postData ?? ""));
fs.writeFileSync(out, JSON.stringify({ url, count: bodies.length, bodies }, null, 2) + "\n");
console.log(`${bodies.length} track requests -> ${out}`);
