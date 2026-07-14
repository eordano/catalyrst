// Generates the bundled asset catalog the scene editor falls back to.
//
// Why a bundle at all: loadAssetCatalog() reads /builder-api/v1/assetPacks,
// which this node answers 404 by design (01-catalyst.conf: anything outside the
// collection/item surface 404s locally rather than proxying to production). Its
// own comment promises the caller "falls back to the bundled seed catalog ...
// instead of showing a browser with nothing in it" -- but the fixture it falls
// back to had zero models, so the Creator Hub's asset browser read
// "0 MODELS / No models available" and nothing could be placed at all.
//
// Source of record is @dcl/asset-packs' catalog.json, vendored with the SDK, so
// these are the official packs rather than anything invented here. Assets are
// content-addressed, and this deployment's own content server already has them:
// /content/contents/<cid> answers 200 same-origin on both catalyst.example.com and
// catalyst.example.com. That is the URL the entries carry -- NOT /builder-items/,
// which returns 501 "not mirrored locally", and not a foreign host, which the
// deployment-portability gate forbids and which would CORS-break every
// self-hoster.
//
// Not every thumbnail is mirrored, though: a subset of the packs' content
// hashes is absent from this content server, so shipping their thumbnailUrl
// made the client fire a 404 per card and lean on the onError glyph fallback.
// Instead of trusting the vendored catalog, the generator PROBES each
// thumbnailUrl against the live content server at generation time and drops
// the ones that are not mirrored (the client then renders the glyph
// fallback deliberately, with no 404). Probing is a network step, so it runs
// in normal generation only; the drift gate (--check) verifies the STORED
// verdicts offline instead of re-hitting the CDN, which would make an
// unattended gate flake on any transient network error.
//
//   node scripts/gen-seed-catalog.mjs           # probe + rewrite the fixture
//   node scripts/gen-seed-catalog.mjs --check   # drift gate (offline)
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

// The SDK vendors the catalog; resolve through the scene template that depends
// on it rather than hardcoding a node_modules path.
const require = createRequire(
  join(REPO, "bevy-explorer/project-realm-template/package.json"),
);
const CATALOG = require.resolve("@dcl/asset-packs/catalog.json");

// A few packs, not all twelve: the fixture is committed and shipped to every
// client, so it buys breadth of *kind* -- props, nature, structures, and the
// smart items the Interact tab needs -- not depth of any one theme.
// Smart Items gets a double budget: the Interact tab's chips (doors, buttons,
// platforms, Seats) filter it by category, and 24 spread round-robin over its
// ~18 categories left one item per shelf.
const PACKS = [
  { name: "Genesis City", take: 24 },
  { name: "Fantasy", take: 24 },
  { name: "Sci-fi", take: 24 },
  { name: "Smart Items", take: 48 },
];

const CONTENT_PREFIX = "/content/contents/";

// The live content server every thumbnailUrl must resolve against. Only the
// same-origin catalyst.example.com/catalyst host is probed (never localhost, never a foreign
// CDN): the client dereferences thumbnailUrl same-origin at runtime, so a pass
// here is the statement the shipped URL is mirrored where it will be fetched.
const PROBE_BASE = "https://catalyst.example.com";
const PROBE_CONCURRENCY = 8; // in-flight requests, so a gate run is not a hammer
const PROBE_TIMEOUT_MS = 5000; // per-request; a hang counts as not-mirrored

// Deterministic, so the same asset keeps its colour across regenerations and the
// drift gate does not fire on noise.
function hueOf(id) {
  let h = 2166136261;
  for (let i = 0; i < id.length; i += 1) {
    h ^= id.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return (h >>> 0) % 360;
}

// The vendored catalog has no `script` field (that is the builder API's smart
// marker); here a smart item is one whose composite wires the asset-packs
// runtime -- Actions/Triggers/States components.
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

// Round-robin across a pack's categories instead of first-N: the Interact tab's
// chips filter by category (doors, buttons, platforms, Seats), and Smart Items
// orders its catalog with whole categories past position 24 -- a head-slice
// would bake a fixture where some chips match nothing.
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

// Single HEAD probe: true only on status 200. A relative thumbnail path is
// resolved against PROBE_BASE (the only shape this deployment emits); a
// full-network failure is treated like a 404 (not mirrored), never as a pass.
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

// Probe many URLs with a bounded in-flight pool. Shared index hands each worker
// a distinct URL; JS runs one worker at a time, so the read-increment never
// races. Returns the set of URLs that resolved live (status 200).
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
      // The full content map plus the GLB's path within it. Without these the
      // editor places the asset by bare src URL and the engine resolves the
      // GLB's RELATIVE references (textures like file1.png) against
      // /content/contents/ -- guaranteed 404s, so models rendered untextured
      // or not at all. With them, placeAssetOnBus pins the map via initAsset
      // and every relative file resolves through the content mapping.
      glbFile,
      contents: asset.contents ?? {},
      // Emitted only when the thumbnail is present in the vendored catalog AND
      // the probe below proves it is mirrored; filtered out otherwise.
      ...(thumb ? { thumbnailUrl: `${CONTENT_PREFIX}${thumb}` } : {}),
      ...(category ? { category } : {}),
      ...(smart ? { smart: true } : {}),
    });
    if (category) categories.add(category);
  }
  if (taken.length === 0) throw new Error(`no .glb assets selected from pack: ${packName}`);
}

// Decide which of the shipped thumbnailUrls are actually mirrored, then drop
// the rest. In normal generation the verdict comes from a live probe; in the
// drift gate it comes from the verdicts already persisted in the artifact
// (offline, deterministic -- the gate never re-hits the CDN).
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
    const { thumbnailUrl, ...rest } = m; // dead upstream -> ship no thumbnailUrl
    return rest;
  }
  return m;
});

models.sort((a, b) => (a.pack === b.pack ? a.name.localeCompare(b.name) : a.pack.localeCompare(b.pack)));

fixture.assetCatalog = {
  categories: [...categories].sort(),
  models,
};
// Persist the probe verdicts so the drift gate can verify them offline: which
// content hash is mirrored is a fact about the live deployment, and re-probing
// an unattended lane on every run would turn a transient CDN error into a
// spurious gate failure. Regenerate (normal mode) to re-probe.
fixture._assetThumbCheck = thumbMeta;
// The fixture must pass scripts/check-ascii.sh: keep non-ASCII as JSON \uXXXX escapes.
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