#!/usr/bin/env node
// foundry-door-probe — the cold-profile walk of the copilot door, on a timer.
//
//   npm run foundry:door-probe   (deploy-foundry-door-probe.timer, hourly)
//
// A fresh Chromium profile recovers the probe persona by return code, mints
// the next code (they are single-use — losing this step locks the probe out
// until an operator re-seeds the code file), walks the site's own door into
// the copilot, and asserts what a first-time visitor needs: a session URL, a
// live composer, the pipeline commands in the slash menu, and the self-hosted
// model selected. No message is sent — the door is the thing under test, and
// the walk must stay cheap enough to run hourly.
//
// The verdict lands atomically in FOUNDRY_DOOR_PROBE_STATUS as JSON the
// copilot page renders verbatim; frames for the last runs sit next to it.
// Every step that fails names itself — a red probe is a page-visible fact.
import { execSync } from "node:child_process";
import { mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

import { freePort, launchChromium, Tab } from "./drive/cdp.mts";

const statusPath = process.env.FOUNDRY_DOOR_PROBE_STATUS?.trim();
if (!statusPath) {
  console.error("foundry-door-probe: FOUNDRY_DOOR_PROBE_STATUS is not set");
  process.exit(2);
}
const codePath = process.env.FOUNDRY_DOOR_PROBE_CODE?.trim() || join(dirname(statusPath), "door-probe-code");
const base = process.env.FOUNDRY_DOOR_PROBE_BASE?.trim() || "https://foundry.catalyst.example.com";
const framesRoot = join(dirname(statusPath), "door-probe-frames");
const KEEP_RUNS = 5;

type Step = { name: string; ok: boolean; detail: string };
const steps: Step[] = [];
const runId = new Date().toISOString().replace(/[:.]/g, "-");
const framesDir = join(framesRoot, runId);
mkdirSync(framesDir, { recursive: true });

function finish(ok: boolean): never {
  const status = { ok, at: new Date().toISOString(), steps, frames: framesDir };
  writeFileSync(statusPath + ".tmp", JSON.stringify(status, null, 2));
  renameSync(statusPath + ".tmp", statusPath);
  const runs = readdirSync(framesRoot).sort();
  for (const old of runs.slice(0, Math.max(0, runs.length - KEEP_RUNS))) {
    rmSync(join(framesRoot, old), { recursive: true, force: true });
  }
  console.log(`foundry-door-probe: ${ok ? "PASS" : "FAIL"} — ${steps.map((s) => `${s.ok ? "+" : "!"}${s.name}`).join(" ")}`);
  process.exit(ok ? 0 : 1);
}

const profile = join(framesDir, "profile");
const port = await freePort();
const chrome = await launchChromium({ port, profileDir: profile });
const tab = await Tab.open(port, "about:blank");
await tab.setViewport(1440, 900);
let shotN = 0;
async function shot(slug: string) {
  shotN++;
  writeFileSync(join(framesDir, `${String(shotN).padStart(2, "0")}-${slug}.png`), Buffer.from(await tab.screenshotB64(), "base64"));
}
async function tclick(x: number, y: number) {
  await tab.cmd("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
  await tab.cmd("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
}
async function rectOf(pred: string): Promise<[number, number] | null> {
  return (await tab.ev(`(() => { const el = ${pred}; if (!el) return null; const b = el.getBoundingClientRect(); return [b.x + b.width/2, b.y + b.height/2]; })()`)) as [number, number] | null;
}
function done(name: string, ok: boolean, detail: string) {
  steps.push({ name, ok, detail });
  if (!ok) {
    void shot(`FAIL-${name}`).then(() => {
      chrome.kill();
      finish(false);
    });
    throw new Error(`step ${name}: ${detail}`);
  }
}

try {
  // 1. recover the probe persona from a cold profile
  const code = readFileSync(codePath, "utf8").trim();
  await tab.navigate(`${base}/foundry/return`, 4500);
  const CODE_INPUT = `[...document.querySelectorAll('input')].find(i => /return code/i.test(i.getAttribute('aria-label') || '') || /^x{5}-/.test(i.placeholder || '') || /code/i.test(i.name || ''))`;
  const filled = await tab.ev(`(() => {
    const el = ${CODE_INPUT};
    if (!el) return 'no code input';
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, ${JSON.stringify(code)});
    el.dispatchEvent(new Event('input', { bubbles: true }));
    return 'filled';
  })()`);
  done("recover-form", filled === "filled", String(filled));
  await tab.ev(`(() => { const el = ${CODE_INPUT}; el?.closest('form')?.requestSubmit(); })()`);
  await new Promise((r) => setTimeout(r, 4500));
  await shot("recovered");
  const persona = String(await tab.ev("document.body.innerText"));
  done("recover", /colda/i.test(persona), /colda/i.test(persona) ? "session re-bound to colda" : "recovery did not land on the persona");

  // 2. mint and persist the next code BEFORE anything else can fail
  await tab.navigate(`${base}/foundry/persona`, 4000);
  await tab.ev(`(() => { const b = [...document.querySelectorAll('button')].find(x => /mint/i.test(x.textContent)); b?.click(); })()`);
  await new Promise((r) => setTimeout(r, 3500));
  const after = String(await tab.ev("document.body.innerText"));
  const next = after.match(/[a-z0-9]{5}-[a-z0-9]{5}-[a-z0-9]{5}-[a-z0-9]{5}/);
  done("rotate-code", Boolean(next), next ? "next return code minted and saved" : "no fresh code on the page — probe will be locked out; re-seed the code file");
  writeFileSync(codePath + ".tmp", next![0] + "\n", { mode: 0o600 });
  renameSync(codePath + ".tmp", codePath);
  await shot("code-rotated");

  // 3. the door
  await tab.navigate(`${base}/foundry/copilot`, 5000);
  const door = await rectOf(`[...document.querySelectorAll('a')].find(e => e.textContent.trim() === 'Open the copilot')`);
  done("door-cta", Boolean(door), door ? "Open the copilot is a live link" : "no door CTA for a host persona");
  await tclick(door![0], door![1]);
  await new Promise((r) => setTimeout(r, 10000));
  const url = String(await tab.ev("location.href"));
  await shot("through-the-door");
  done("door-lands-in-session", /\/session\/ses_/.test(url), url);

  // 4. the composer a first-time visitor gets
  const ta = await rectOf(`document.querySelector('textarea, [contenteditable=true]')`);
  done("composer", Boolean(ta), ta ? "composer present" : "no composer in the session view");
  await tclick(ta![0], ta![1]);
  await tab.cmd("Input.insertText", { text: "/pipe" });
  await new Promise((r) => setTimeout(r, 2500));
  await shot("slash-menu");
  const body = String(await tab.ev("document.body.innerText"));
  done("pipeline-commands", body.includes("pipeline-intake"), body.includes("pipeline-intake") ? "slash menu lists the pipeline steps" : "pipeline commands missing from the slash menu");
  done("model", /llm.default/i.test(body), /llm.default/i.test(body) ? "model chip reads llm-default (self-hosted)" : body.includes("Big Pickle") ? "composer fell back to Big Pickle — session is outside the workspace" : "model chip unreadable");

  chrome.kill();
  finish(true);
} catch (err) {
  try {
    chrome.kill();
  } catch {
    /* already down */
  }
  if (steps.length === 0 || steps[steps.length - 1].ok) {
    steps.push({ name: "crash", ok: false, detail: String(err).slice(0, 200) });
  }
  finish(false);
}
