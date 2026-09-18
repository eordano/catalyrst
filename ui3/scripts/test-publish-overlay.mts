import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, existsSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = mkdtempSync(join(tmpdir(), 'overlay-retention-'));
const target = join(root, 'served');
const source = join(root, 'built');
function generation(dir: string, name: string) {
  mkdirSync(join(dir, 'chunks'), { recursive: true });
  writeFileSync(join(dir, 'overlay.js'), `import './chunks/${name}.js';`);
  writeFileSync(join(dir, 'chunks', `${name}.js`), 'export {};');
}
function rotate() {
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('./publish-overlay.mts', import.meta.url)), '--rotate'], {
    env: { ...process.env, OVERLAY_SOURCE: source, OVERLAY_TARGET: target }, encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stdout + result.stderr);
}
try {
  generation(target, 'old');
  generation(source, 'current');
  rotate();
  assert.ok(existsSync(join(target, 'chunks/old.js')), 'First publish retains chunks from a build without a ledger');
  assert.ok(existsSync(join(target, 'chunks/current.js')));
  rmSync(source, { recursive: true });
  generation(source, 'next');
  rotate();
  assert.ok(!existsSync(join(target, 'chunks/old.js')), 'Second publish prunes the oldest generation');
  assert.ok(existsSync(join(target, 'chunks/current.js')));
  assert.ok(existsSync(join(target, 'chunks/next.js')));
  assert.deepEqual(JSON.parse(readFileSync(join(target, '.publish-manifest.json'), 'utf8')).previous, ['chunks/current.js']);
  console.log('PASS: first publish preserves old chunks; subsequent publish retains exactly one previous generation');
} finally { rmSync(root, { recursive: true, force: true }); }
