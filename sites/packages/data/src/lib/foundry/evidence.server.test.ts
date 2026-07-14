import { promises as fs } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";
import { describe, expect, it } from "vitest";

import {
  inspectEvidenceDir,
  redactEvidenceText,
  safeEvidenceFile,
} from "./evidence.server";

describe("inspectEvidenceDir log redaction", () => {
  it("strips the run root from the log tail, not just the evidence dir", async () => {
    // The on-disk shape the harness leaves behind: <root>/evidence/<name> next
    // to <root>/manifests, with run.log quoting both by absolute path.
    const root = await fs.mkdtemp(join(tmpdir(), "fd-evidence-"));
    try {
      const dir = join(root, "evidence", "alienscrapyard-smoke");
      await fs.mkdir(dir, { recursive: true });
      await fs.writeFile(
        join(dir, "run.log"),
        [
          `re-derive: python3 -m dclbots.run ${join(root, "manifests", "x.json")} --replay ${join(dir, "snapshot.json")}`,
          "done",
          "",
        ].join("\n"),
        "utf8",
      );

      const listing = await inspectEvidenceDir(dir);
      expect(listing.present).toBe(true);
      const tail = (listing.logTail ?? []).join("\n");
      expect(tail).toContain("manifests/x.json");
      expect(tail).toContain("alienscrapyard-smoke/snapshot.json");
      expect(tail).not.toContain("/tmp/");
      expect(tail).not.toContain("claude");
    } finally {
      await fs.rm(root, { recursive: true, force: true });
    }
  });

  it("counts the whole log after the trailing-blank trim", async () => {
    const root = await fs.mkdtemp(join(tmpdir(), "fd-evidence-"));
    try {
      const dir = join(root, "evidence", "count-run");
      await fs.mkdir(dir, { recursive: true });
      await fs.writeFile(
        join(dir, "run.log"),
        ["one", "two", "three", "", ""].join("\n"),
        "utf8",
      );
      const listing = await inspectEvidenceDir(dir);
      expect(listing.logLines).toBe(3);
      expect(listing.logTail).toEqual(["one", "two", "three"]);
    } finally {
      await fs.rm(root, { recursive: true, force: true });
    }
  });

  it("reports logLines null with the tail when run.log is absent", async () => {
    const root = await fs.mkdtemp(join(tmpdir(), "fd-evidence-"));
    try {
      const listing = await inspectEvidenceDir(root);
      expect(listing.present).toBe(true);
      expect(listing.logTail).toBeNull();
      expect(listing.logLines).toBeNull();
    } finally {
      await fs.rm(root, { recursive: true, force: true });
    }
  });
});

describe("redactEvidenceText", () => {
  it("replaces the evidence dir with its basename and strips the run root", () => {
    const dir = join(sep, "scratch", "runs", "r1", "evidence", "smoke-run");
    const runRoot = resolve(dir, "..", "..") + sep;
    const text = [
      `wrote ${join(dir, "snapshot.json")}`,
      `read ${runRoot}manifests${sep}x.json`,
    ].join("\n");
    const out = redactEvidenceText(dir, text);
    expect(out).toContain(`smoke-run${sep}snapshot.json`);
    expect(out).toContain(`manifests${sep}x.json`);
    expect(out).not.toContain(dir);
    expect(out).not.toContain(runRoot);
  });
});

describe("safeEvidenceFile", () => {
  const dir = join(tmpdir(), "fd-evidence-safe");

  it.each([
    "../x",
    "shots/../../etc/passwd",
    "/etc/passwd",
    ".hidden",
    "",
    " .png",
    "a/%2e%2e/x",
    "x/",
  ])("refuses %j", (rel) => {
    expect(safeEvidenceFile(dir, rel)).toBeNull();
  });

  it.each(["run.log", "shots/frame-001.png"])(
    "maps %j inside the directory",
    (rel) => {
      const abs = safeEvidenceFile(dir, rel);
      expect(abs).not.toBeNull();
      expect(abs!.startsWith(resolve(dir) + sep)).toBe(true);
    },
  );
});
