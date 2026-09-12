import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = join(HERE, "..", "..", "..");
const FIXTURE = join(
  REPO,
  "catalyrst/sites/packages/data/src/lib/catalyst/creator-hub/scene-editor-defaults.data.json",
);

const require = createRequire(
  join(REPO, "bevy-explorer/project-realm-template/package.json"),
);
const CATALOG = require.resolve("@dcl/asset-packs/catalog.json");

const PACKS = [
  { name: "Genesis City", take: 24 },
  { name: "Fantasy", take: 24 },
  { name: "Sci-fi", take: 24 },
  { name: "Smart Items", take: 48 },
];

const CONTENT_PREFIX = "/content/contents/";

const PROBE_BASE = "https://catalyst.example.com";
const PROBE_CONCURRENCY = 8;
const PROBE_TIMEOUT_MS = 5000;

function hueOf(id) {
  let h = 2166136261;
  for (let i = 0; i < id.length; i += 1) {
    h ^= id.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return (h >>> 0) % 360;
}

function isSmart(asset) {
  const components = asset.composite?.components;
  if (!Array.isArray(components)) return false;
  return components.some((c) => /^asset-packs::(Actions|Triggers|States)$/.test(c?.name ?? ""));
}

function glbOf(asset) {
  const entries = Object.entries(asset.contents ?? {});
  const glb = entries.find(([file]) => /\.glb$/i.test(file));
  return glb ?? null;
}

function thumbOf(asset) {
  const hash = (asset.contents ?? {})["thumbnail.png"];
  return typeof hash === "string" && hash ? hash : null;
}

function samplePack(assets, limit) {
  const byCategory = new Map();
  for (const asset of assets) {
    if (glbOf(asset) === null) continue;
    const key = (asset.category ?? "").trim();
    if (key === "deprecated") continue;
    if (!byCategory.has(key)) byCategory.set(key, []);
    byCategory.get(key).push(asset);
  }
  const buckets = [...byCategory.values()];
  const taken = [];
  for (let round = 0; taken.length < limit; round += 1) {
    let any = false;
    for (const bucket of buckets) {
      if (taken.length >= limit) break;
      if (round < bucket.length) {
        taken.push(bucket[round]);
        any = true;
      }
    }
    if (!any) break;
  }
  return taken;
}

async function probeOne(url, timeoutMs) {
  const abs = new URL(url, PROBE_BASE);
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const res = await fetch(abs, { method: "HEAD", signal: ctrl.signal, redirect: "follow" });
    return res.status === 200;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}

async function probeThumbs(urls) {
  const live = new Set();
  let idx = 0;
  async function worker() {
    for (;;) {
      const i = idx++;
      if (i >= urls.length) return;
      const url = urls[i];
      if (await probeOne(url, PROBE_TIMEOUT_MS)) live.add(url);
    }
  }
  const pool = Array.from({ length: Math.min(PROBE_CONCURRENCY, urls.length) }, () => worker());
  await Promise.all(pool);
  return live;
}

const catalog = JSON.parse(readFileSync(CATALOG, "utf8"));
const isCheck = process.argv.includes("--check");

const fixtureText = readFileSync(FIXTURE, "utf8");
const fixture = JSON.parse(fixtureText);

const candidates = [];
const categories = new Set();

for (const { name: packName, take } of PACKS) {
  const pack = (catalog.assetPacks ?? []).find((p) => p.name === packName);
  if (!pack) throw new Error(`asset pack not found in catalog.json: ${packName}`);
  const taken = samplePack(pack.assets ?? [], take);
  for (const asset of taken) {
    const [glbFile, hash] = glbOf(asset);
    const category = (asset.category ?? "").trim();
    const smart = isSmart(asset);
    const thumb = thumbOf(asset);
    candidates.push({
      id: asset.id,
      name: asset.name,
      pack: pack.name,
      src: `${CONTENT_PREFIX}${hash}`,
      hue: hueOf(asset.id),
      glbFile,
      contents: asset.contents ?? {},
      ...(thumb ? { thumbnailUrl: `${CONTENT_PREFIX}${thumb}` } : {}),
      ...(category ? { category } : {}),
      ...(smart ? { smart: true } : {}),
    });
    if (category) categories.add(category);
  }
  if (taken.length === 0) throw new Error(`no .glb assets selected from pack: ${packName}`);
}

let liveSet;
let thumbMeta;
if (isCheck) {
  thumbMeta = fixture._assetThumbCheck;
  if (!thumbMeta || typeof thumbMeta !== "object" || !Array.isArray(thumbMeta.live)) {
    console.error("gen-seed-catalog: fixture has no _assetThumbCheck verdicts to verify against");
    console.error("run: node scripts/gen-seed-catalog.mjs   (network: establishes the probe verdicts)");
    process.exit(1);
  }
  liveSet = new Set(thumbMeta.live);
} else {
  const toProbe = [...new Set(candidates.map((m) => m.thumbnailUrl).filter(Boolean))].sort();
  liveSet = await probeThumbs(toProbe);
  thumbMeta = {
    base: PROBE_BASE,
    checkedAt: new Date().toISOString(),
    live: [...liveSet].sort(),
  };
}

let total = 0;
let ok = 0;
let nulled = 0;
const models = candidates.map((m) => {
  if (m.thumbnailUrl) {
    total += 1;
    if (liveSet.has(m.thumbnailUrl)) {
      ok += 1;
      return m;
    }
    nulled += 1;
    const { thumbnailUrl, ...rest } = m;
    return rest;
  }
  return m;
});

models.sort((a, b) => (a.pack === b.pack ? a.name.localeCompare(b.name) : a.pack.localeCompare(b.pack)));

fixture.assetCatalog = {
  categories: [...categories].sort(),
  models,
};
fixture._assetThumbCheck = thumbMeta;
const next = `${JSON.stringify(fixture, null, 2).replace(/[^\x00-\x7f]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`)}\n`;

if (isCheck) {
  if (fixtureText !== next) {
    console.error("gen-seed-catalog: scene-editor-defaults.data.json is stale");
    console.error("run: node scripts/gen-seed-catalog.mjs");
    process.exit(1);
  }
  console.log(
    `gen-seed-catalog: fixture matches (${models.length} models, ${categories.size} categories; ` +
      `thumbnails ${total} total / ${ok} live / ${nulled} nulled -- stored verdicts, checked offline)`,
  );
} else {
  writeFileSync(FIXTURE, next);
  console.log(
    `gen-seed-catalog: wrote ${models.length} models from ${PACKS.length} packs, ${categories.size} categories`,
  );
  console.log(
    `gen-seed-catalog: thumbnails ${total} total / ${ok} live / ${nulled} nulled (HEAD ${PROBE_BASE}, ` +
      `dropped not-mirrored so the client renders the glyph, not a 404)`,
  );
}