#!/usr/bin/env node
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { freePort, launchChromium, Tab } from "./cdp.mts";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BASELINES = path.join(HERE, "baselines");

type Viewport = [number, number];

type Opts = {
  base: string;
  sbBase: string;
  update: boolean;
  viewports: Viewport[];
  wait: number;
  threshold: number;
  burner: boolean;
  out: string;
  targets: string[];
};

function parseArgs(argv: string[]): Opts {
  const opts: Opts = {
    base: "https://catalyst.example.com",
    sbBase: "http://localhost:5006",
    update: false,
    viewports: [[1440, 900]],
    wait: 3500,
    threshold: 0.5,
    burner: false,
    out: path.join(HERE, "out"),
    targets: [],
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--update") opts.update = true;
    else if (a === "--burner") opts.burner = true;
    else if (a === "--base") opts.base = argv[++i]!;
    else if (a === "--sb-base") opts.sbBase = argv[++i]!;
    else if (a === "--wait") opts.wait = Number(argv[++i]);
    else if (a === "--threshold") opts.threshold = Number(argv[++i]);
    else if (a === "--out") opts.out = argv[++i]!;
    else if (a === "--viewports") {
      opts.viewports = argv[++i]!
        .split(",")
        .map((v) => v.split("x").map(Number) as Viewport);
    } else opts.targets.push(a);
  }
  if (!opts.targets.length) {
    console.error("usage: shot.mts [flags] <path-or-url> [...]");
    process.exit(2);
  }
  return opts;
}

function gitStamp(): { commit: string; dirty: boolean; stamp: string } {
  const run = (cmd: string): string => execSync(cmd, { encoding: "utf8" }).trim();
  const commit = run("git rev-parse --short HEAD");
  const dirty =
    run("git status --porcelain -- sites ui3 | head -50").length > 0;
  return { commit, dirty, stamp: `${commit}${dirty ? "+dirty" : ""}` };
}

function slug(target: string, [w, h]: Viewport): string {
  return (
    target
      .replace(/^https?:\/\/[^/]+/, "")
      .replace(/[^a-zA-Z0-9]+/g, "-")
      .replace(/^-|-$/g, "") || "root"
  ) + `@${w}x${h}`;
}

const DIFF_FN = `async (a64, b64) => {
  const load = (b64) => new Promise((res, rej) => {
    const img = new Image();
    img.onload = () => res(img);
    img.onerror = rej;
    img.src = 'data:image/png;base64,' + b64;
  });
  const [ia, ib] = await Promise.all([load(a64), load(b64)]);
  const w = Math.max(ia.width, ib.width), h = Math.max(ia.height, ib.height);
  const cv = (img) => {
    const c = new OffscreenCanvas(w, h);
    const ctx = c.getContext('2d');
    ctx.drawImage(img, 0, 0);
    return ctx.getImageData(0, 0, w, h).data;
  };
  const da = cv(ia), db = cv(ib);
  const out = new OffscreenCanvas(w, h);
  const octx = out.getContext('2d');
  const od = octx.createImageData(w, h);
  let diff = 0;
  for (let i = 0; i < da.length; i += 4) {
    const delta = Math.max(
      Math.abs(da[i] - db[i]), Math.abs(da[i+1] - db[i+1]),
      Math.abs(da[i+2] - db[i+2]));
    if (delta > 12) {
      diff++;
      od.data[i] = 255; od.data[i+3] = 255;
    } else {
      od.data[i] = da[i]; od.data[i+1] = da[i+1];
      od.data[i+2] = da[i+2]; od.data[i+3] = 60;
    }
  }
  octx.putImageData(od, 0, 0);
  const blob = await out.convertToBlob({ type: 'image/png' });
  const buf = await blob.arrayBuffer();
  let bin = '';
  const bytes = new Uint8Array(buf);
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
  }
  return { pct: (100 * diff) / (w * h), diffB64: btoa(bin) };
}`;

type ShotRow = {
  target: string;
  viewport: string;
  shot: string;
  baseline?: string;
  diffPct?: number;
  diff?: string;
};

async function main(): Promise<void> {
  const opts = parseArgs(process.argv.slice(2));
  const git = gitStamp();
  fs.mkdirSync(opts.out, { recursive: true });
  fs.mkdirSync(BASELINES, { recursive: true });

  const port = await freePort();
  const profile = fs.mkdtempSync("/tmp/ui-shot-");
  console.log(`[ui-shot] chromium on :${port} \u{B7} base=${opts.base} \u{B7} ${git.stamp}`);
  const chromium = await launchChromium({ port, profileDir: profile });

  const results: ShotRow[] = [];
  try {
    const tab = await Tab.open(port);
    if (opts.burner) {
      await tab.navigate(opts.base + "/", 2500);
      const ok = await tab.ev(
        "window.__DCL_DEV__ ? window.__DCL_DEV__.signInBurner().then(() => true) : false",
        { awaitPromise: true },
      );
      console.log(`[ui-shot] burner sign-in: ${ok ? "ok" : "UNAVAILABLE (no __DCL_DEV__ \u{2014} not a dev server?)"}`);
    }
    for (const target of opts.targets) {
      let url: string;
      let nameBase = target;
      if (target.startsWith("story:")) {
        const id = target.slice("story:".length);
        url = `${opts.sbBase}/iframe.html?id=${encodeURIComponent(id)}&viewMode=story`;
        nameBase = `story-${id}`;
      } else {
        url = target.startsWith("http") ? target : opts.base + target;
      }
      for (const [w, h] of opts.viewports) {
        await tab.setViewport(w, h);
        await tab.navigate(url, opts.wait);
        const b64 = await tab.screenshotB64();
        const name = slug(nameBase, [w, h]);
        const shotFile = path.join(opts.out, `${name}.png`);
        fs.writeFileSync(shotFile, Buffer.from(b64, "base64"));
        const baseFile = path.join(BASELINES, `${name}.png`);
        const row: ShotRow = { target, viewport: `${w}x${h}`, shot: shotFile };

        if (opts.update) {
          fs.copyFileSync(shotFile, baseFile);
          row.baseline = "updated";
        } else if (fs.existsSync(baseFile)) {
          const base64 = fs.readFileSync(baseFile).toString("base64");
          const { pct, diffB64 } = await tab.ev(
            `(${DIFF_FN})(${JSON.stringify(base64)}, ${JSON.stringify(b64)})`,
            { awaitPromise: true, timeoutMs: 60_000 },
          );
          row.diffPct = Number(pct.toFixed(3));
          if (pct > opts.threshold) {
            const diffFile = path.join(opts.out, `${name}.diff.png`);
            fs.writeFileSync(diffFile, Buffer.from(diffB64, "base64"));
            row.diff = diffFile;
          }
        } else {
          row.baseline = "MISSING (run with --update to create)";
        }
        results.push(row);
        console.log(
          `[ui-shot] ${name}: ` +
            (row.diffPct !== undefined
              ? `diff ${row.diffPct}%${row.diff ? ` -> ${row.diff}` : ""}`
              : row.baseline),
        );
      }
    }
  } finally {
    const exited = new Promise((r) => chromium.once("exit", r));
    chromium.kill();
    await Promise.race([exited, new Promise((r) => setTimeout(r, 4000))]);
    try {
      fs.rmSync(profile, { recursive: true, force: true });
    } catch {
    }
  }

  const manifest = { base: opts.base, git, results };
  fs.writeFileSync(
    path.join(opts.out, "manifest.json"),
    JSON.stringify(manifest, null, 2),
  );
  const failures = results.filter(
    (r) => r.diffPct !== undefined && r.diffPct > opts.threshold,
  );
  if (failures.length && !opts.update) {
    console.error(`[ui-shot] ${failures.length} shot(s) over threshold ${opts.threshold}%`);
    process.exit(1);
  }
}

main().catch((err) => {
  console.error("[ui-shot]", err);
  process.exit(1);
});
