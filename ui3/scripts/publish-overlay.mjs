#!/usr/bin/env node
import { createHash } from "node:crypto";
import {
    copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync,
    rmdirSync, rmSync, statSync, unlinkSync, watch, writeFileSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const UI3 = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DIST = join(UI3, "dist-overlay");
const TARGET = resolve(UI3, "../../bevy-explorer/deploy/web/ui3-overlay");
if (!existsSync(resolve(UI3, "../../bevy-explorer"))) {
    console.log("publish-overlay: no ../../bevy-explorer sibling checkout — nothing to publish");
    process.exit(0);
}
const MANIFEST_NAME = ".publish-manifest.json";
const LOCK = join(process.env.HOME ?? "/tmp", ".overlay-publish-lock");
const PIN = resolve(UI3, "../deploy/play-artifact.nix");

const args = new Set(process.argv.slice(2));
const CHECK = args.has("--check");
const WATCH = args.has("--watch");
const PRUNE_ALL = args.has("--prune-all");

function walk(dir, base = dir) {
    const out = [];
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = join(dir, entry.name);
        if (entry.isDirectory()) out.push(...walk(p, base));
        else out.push(relative(base, p));
    }
    return out;
}

const sha256 = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");

const retainable = (rel) => rel.startsWith("chunks/") || rel.startsWith("assets/");

function readManifest() {
    const p = join(TARGET, MANIFEST_NAME);
    if (!existsSync(p)) return { current: {}, previous: [] };
    try {
        return JSON.parse(readFileSync(p, "utf8"));
    } catch {
        return { current: {}, previous: [] };
    }
}

function freshFileset() {
    if (!existsSync(join(DIST, "overlay.js"))) {
        console.error(`publish-overlay: ${DIST} has no overlay.js — run the vite build first`);
        process.exit(2);
    }
    const set = {};
    for (const rel of walk(DIST)) set[rel] = sha256(join(DIST, rel));
    return set;
}

function atomicCopy(src, dest) {
    mkdirSync(dirname(dest), { recursive: true });
    const tmp = `${dest}.tmp-${process.pid}`;
    copyFileSync(src, tmp);
    renameSync(tmp, dest);
}

function pruneEmptyDirs(dir) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
        if (!entry.isDirectory()) continue;
        const p = join(dir, entry.name);
        pruneEmptyDirs(p);
        if (readdirSync(p).length === 0) rmdirSync(p);
    }
}

// The /play pin covers the HUD via the pin's `overlay` block; every publish
// rewrites its hashes so deploy-dev-health can tell a published HUD from a
// swapped one. Missing anchors warn rather than corrupt the pin.
function bumpPin(fresh) {
    if (!existsSync(PIN)) {
        console.error(`publish-overlay: WARN pin ${PIN} missing — overlay hashes not recorded`);
        return;
    }
    let src = readFileSync(PIN, "utf8");
    let changed = false;
    for (const rel of ["overlay.js", "overlay.css"]) {
        if (!(rel in fresh)) continue;
        const key = `"ui3-overlay/${rel}"`;
        const line = new RegExp(`^(\\s*${key.replace(/[./]/g, "\\$&")} *= *")sha256-[0-9a-f]+(";)$`, "m");
        if (!line.test(src)) {
            console.error(`publish-overlay: WARN pin has no ${key} line — not bumped`);
            continue;
        }
        src = src.replace(line, `$1sha256-${fresh[rel]}$2`);
        changed = true;
    }
    if (changed) {
        src = src.replace(/^(\s*bumped *= *")[^"]*(";)$/m, `$1${new Date().toISOString().slice(0, 10)}$2`);
        const tmp = `${PIN}.tmp-${process.pid}`;
        writeFileSync(tmp, src);
        renameSync(tmp, PIN);
        console.log(`publish-overlay: pin overlay hashes bumped (${PIN})`);
    }
}

function sync({ prune }) {
    const fresh = freshFileset();
    const manifest = readManifest();
    let copied = 0;
    for (const [rel, hash] of Object.entries(fresh)) {
        const dest = join(TARGET, rel);
        if (existsSync(dest) && sha256(dest) === hash) continue;
        atomicCopy(join(DIST, rel), dest);
        copied++;
    }
    if (!prune) {
        if (copied > 0) bumpPin(fresh);
        console.log(`publish-overlay: synced ${copied} file(s) (watch mode, no prune)`);
        return;
    }
    const retained = PRUNE_ALL
        ? []
        : Object.keys(manifest.current).filter((rel) => retainable(rel) && !(rel in fresh));
    const keep = new Set([...Object.keys(fresh), ...retained, MANIFEST_NAME]);
    let pruned = 0;
    for (const rel of walk(TARGET)) {
        if (keep.has(rel)) continue;
        unlinkSync(join(TARGET, rel));
        pruned++;
    }
    pruneEmptyDirs(TARGET);
    const next = { generatedAt: new Date().toISOString(), current: fresh, previous: retained };
    const tmp = join(TARGET, `${MANIFEST_NAME}.tmp-${process.pid}`);
    writeFileSync(tmp, JSON.stringify(next, null, 2) + "\n");
    renameSync(tmp, join(TARGET, MANIFEST_NAME));
    bumpPin(fresh);
    console.log(
        `publish-overlay: ${copied} copied, ${retained.length} previous-gen retained, ${pruned} pruned → ${TARGET}`,
    );
}

function check() {
    const fresh = freshFileset();
    const manifest = readManifest();
    const problems = [];
    for (const [rel, hash] of Object.entries(fresh)) {
        const dest = join(TARGET, rel);
        if (!existsSync(dest)) problems.push(`MISSING  ${rel}`);
        else if (sha256(dest) !== hash) problems.push(`DIFFERS  ${rel}`);
    }
    const previous = new Set(manifest.previous ?? []);
    for (const rel of walk(TARGET)) {
        if (rel === MANIFEST_NAME || rel in fresh) continue;
        if (!(previous.has(rel) && retainable(rel))) problems.push(`STALE    ${rel}`);
    }
    for (const [rel, hash] of Object.entries(manifest.current ?? {})) {
        if (fresh[rel] !== hash) {
            problems.push(`MANIFEST ${rel} (ledger disagrees with fresh build)`);
        }
    }
    if (Object.keys(manifest.current ?? {}).length !== Object.keys(fresh).length) {
        problems.push("MANIFEST fileset size disagrees with fresh build");
    }
    if (problems.length) {
        for (const p of problems) console.error(`DRIFT: ${p}`);
        console.error(
            `publish-overlay: ${problems.length} drift(s) — run \`npm run overlay:publish\` and commit`,
        );
        process.exit(1);
    }
    console.log("publish-overlay: OK (served tree matches fresh build + retention ledger)");
}

function withLock(fn) {
    try {
        mkdirSync(LOCK);
    } catch {
        console.error(`publish-overlay: lock ${LOCK} busy — another publisher is running`);
        process.exit(3);
    }
    try {
        fn();
    } finally {
        rmSync(LOCK, { recursive: true, force: true });
    }
}

if (WATCH) {
    const vite = spawn("npx", ["vite", "build", "--watch", "--config", "vite.overlay.config.js"], {
        cwd: UI3,
        stdio: "inherit",
    });
    let timer = null;
    mkdirSync(DIST, { recursive: true });
    watch(DIST, { recursive: true }, () => {
        clearTimeout(timer);
        timer = setTimeout(() => {
            try {
                sync({ prune: false });
            } catch (e) {
                console.error(`publish-overlay: watch sync failed: ${e.message}`);
            }
        }, 300);
    });
    process.on("SIGINT", () => {
        vite.kill("SIGINT");
        process.exit(0);
    });
} else if (CHECK) {
    check();
} else {
    withLock(() => sync({ prune: true }));
}
