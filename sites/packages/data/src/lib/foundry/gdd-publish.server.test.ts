import { describe, expect, it } from "vitest";

import { draftFromTranscript } from "./gdd-publish.server";
import { parseGddText } from "./gdd.server";

const DOC = [
  "---",
  "title: Flagrush",
  "---",
  "",
  "## 0. TL;DR",
  "High-velocity CTF.",
  "## 1. The Hook",
  "Like CTF, but TBD: faster.",
  "## 2. First Session",
  "You run.",
  "## 3. Core Loop",
  "Steal, return. [agent-decided]",
  "## 4. Why Players Come Back",
  "Cosmetics.",
].join("\n");

const msg = (
  role: string,
  completed: boolean,
  ...texts: string[]
): Record<string, unknown> => ({
  info: { role, time: completed ? { completed: 1 } : {} },
  parts: texts.map((t) => ({ type: "text", text: t })),
});

const toolMsg = (
  completed: boolean,
  output: string,
  tool = "docsmith_emit_document",
  status = "completed",
) => ({
  info: { role: "assistant", time: completed ? { completed: 1 } : {} },
  parts: [{ type: "tool", tool, state: { status, output } }],
});

describe("draftFromTranscript", () => {
  it("takes a completed emit_document tool output, SAVED preamble stripped", () => {
    const ms = [toolMsg(true, "SAVED to projects/x/design/shortGDD.md (1 bytes).\n\n---\ntitle: X\n---\n\n" + DOC)];
    const got = draftFromTranscript(ms);
    expect(got).not.toBeNull();
    expect(got!.startsWith("---\ntitle: X")).toBe(true);
    expect(got).toContain("## ");
  });

  it("prefers the tool emission over an older fenced block, and skips failed or foreign tools", () => {
    const ms = [
      msg("assistant", true, "```markdown\n" + DOC + "\n```"),
      toolMsg(true, "NOT SAVED — problems", "docsmith_emit_document"),
      toolMsg(true, "---\ntitle: T\n---\n\n" + DOC),
    ];
    expect(draftFromTranscript(ms)!.startsWith("---\ntitle: T")).toBe(true);
    const foreign = [toolMsg(true, "---\ntitle: F\n---\n\n" + DOC, "read")];
    expect(draftFromTranscript(foreign)).toBeNull();
    const pending = [toolMsg(true, "---\ntitle: P\n---\n\n" + DOC, "docsmith_emit_document", "running")];
    expect(draftFromTranscript(pending)).toBeNull();
  });

  it("finds the fenced document in a completed assistant message", () => {
    const ms = [
      msg("user", true, "make me a game"),
      msg("assistant", true, "Here it is:\n```markdown\n" + DOC + "\n```"),
    ];
    expect(draftFromTranscript(ms)).toBe(DOC);
  });

  it("prefers the newest document when the session revised it", () => {
    const v2 = DOC.replace("High-velocity CTF.", "REVISED.");
    const ms = [
      msg("assistant", true, "```markdown\n" + DOC + "\n```"),
      msg("assistant", true, "revised:\n```markdown\n" + v2 + "\n```"),
    ];
    expect(draftFromTranscript(ms)).toContain("REVISED.");
  });

  it("skips incomplete messages, code samples, and tiny blocks", () => {
    const ms = [
      msg("assistant", false, "```markdown\n" + DOC + "\n```"),
      msg("assistant", true, "```\nconst x = 1;\n```"),
      msg("assistant", true, "```markdown\n## 1. Alone\nnot a doc\n```"),
    ];
    expect(draftFromTranscript(ms)).toBeNull();
  });

  it("is null on junk", () => {
    expect(draftFromTranscript(null)).toBeNull();
    expect(draftFromTranscript({})).toBeNull();
    expect(draftFromTranscript([])).toBeNull();
  });
});

describe("parseGddText for transcript publishes", () => {
  it("stamps source and sourceRef and counts honesty per section", () => {
    const doc = parseGddText(DOC, {
      source: "copilot",
      sourceRef: "copilot session ses_x",
    });
    expect(doc.id).toBe("flagrush-v1");
    expect(doc.source).toBe("copilot");
    expect(doc.sourceRef).toBe("copilot session ses_x");
    expect(doc.honesty.sections).toHaveLength(5);
    expect(doc.hypotheses).toEqual([]);
  });

  it("refuses a document with no title anywhere", () => {
    expect(() => parseGddText("## 0. TL;DR\nno frontmatter")).toThrow(/title/);
  });
});
