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
//   node scripts/gen-seed-catalog.mjs           # rewrite the fixture
//   node scripts/gen-seed-catalog.mjs --check   # drift gate
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

const catalog = JSON.parse(readFileSync(CATALOG, "utf8"));
const models = [];
const categories = new Set();

for (const { name: packName, take } of PACKS) {
  const pack = (catalog.assetPacks ?? []).find((p) => p.name === packName);
  if (!pack) throw new Error(`asset pack not found in catalog.json: ${packName}`);
  const taken = samplePack(pack.assets ?? [], take);
  for (const asset of taken) {
    const [, hash] = glbOf(asset);
    const category = (asset.category ?? "").trim();
    const smart = isSmart(asset);
    models.push({
      id: asset.id,
      name: asset.name,
      pack: pack.name,
      src: `${CONTENT_PREFIX}${hash}`,
      hue: hueOf(asset.id),
      ...(category ? { category } : {}),
      ...(smart ? { smart: true } : {}),
    });
    if (category) categories.add(category);
  }
  if (taken.length === 0) throw new Error(`no .glb assets selected from pack: ${packName}`);
}

models.sort((a, b) => (a.pack === b.pack ? a.name.localeCompare(b.name) : a.pack.localeCompare(b.pack)));

const fixtureText = readFileSync(FIXTURE, "utf8");
const fixture = JSON.parse(fixtureText);
fixture.assetCatalog = {
  categories: [...categories].sort(),
  models,
};
const next = `${JSON.stringify(fixture, null, 2)}\n`;

if (process.argv.includes("--check")) {
  if (fixtureText !== next) {
    console.error("gen-seed-catalog: scene-editor-defaults.data.json is stale");
    console.error("run: node scripts/gen-seed-catalog.mjs");
    process.exit(1);
  }
  console.log(`gen-seed-catalog: fixture matches (${models.length} models, ${categories.size} categories)`);
} else {
  writeFileSync(FIXTURE, next);
  console.log(
    `gen-seed-catalog: wrote ${models.length} models from ${PACKS.length} packs, ${categories.size} categories`,
  );
}
