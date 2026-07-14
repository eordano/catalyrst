import { describe, expect, it } from "vitest";

import { editedGddDoc, nextGddId } from "./gdd-edit.server";
import { parseGddText, replaceGddSection, splitGddSections } from "./gdd.server";
import type { GddDoc } from "./types";

const BODY = [
  "Preamble the split never returns — it must survive edits untouched.",
  "",
  "##   0. TL;DR  ",
  "High-velocity CTF. [OPEN]",
  "",
  "## 1. The Hook",
  "Like CTF, but TBD: faster.",
  "## 2. Core Loop",
  "Steal, return.",
].join("\n");

const DOC = ["---", "title: Flagrush", "version: 3", "---", "", BODY].join("\n");

function doc(): GddDoc {
  return { ...parseGddText(DOC), updatedAt: "2026-08-17T00:00:00.000Z" };
}

describe("replaceGddSection", () => {
  it("splices one section and leaves every other byte alone", () => {
    const out = replaceGddSection(BODY, 1, "A sharper hook.");
    expect(out).toContain("Preamble the split never returns");
    expect(out).toContain("##   0. TL;DR  ");
    expect(out).toContain("High-velocity CTF. [OPEN]");
    expect(out).toContain("## 1. The Hook\nA sharper hook.");
    expect(out).toContain("## 2. Core Loop\nSteal, return.");
    expect(out).not.toContain("TBD: faster");
  });

  it("replaces the last section to end of document", () => {
    const out = replaceGddSection(BODY, 2, "Give, take.");
    expect(out.endsWith("## 2. Core Loop\nGive, take.")).toBe(true);
  });

  it("round-trips with splitGddSections", () => {
    const sections = splitGddSections(BODY);
    const out = replaceGddSection(BODY, 0, "Rewritten summary.");
    const resplit = splitGddSections(out);
    expect(resplit.map((s) => s.name)).toEqual(sections.map((s) => s.name));
    expect(resplit[0].contentMd).toBe("Rewritten summary.");
    expect(resplit[2].contentMd).toBe(sections[2].contentMd);
  });

  it("normalizes CRLF in the new content", () => {
    const out = replaceGddSection(BODY, 0, "a\r\nb");
    expect(out).toContain("a\nb");
    expect(out).not.toContain("\r");
  });

  it("throws on a section the document does not have", () => {
    expect(() => replaceGddSection(BODY, 9, "x")).toThrow(/no section 9/);
  });
});

describe("nextGddId", () => {
  it("bumps a vN suffix in place", () => {
    expect(nextGddId("alien-human-zoo-shortgdd-v1", 2)).toBe(
      "alien-human-zoo-shortgdd-v2",
    );
    expect(nextGddId("zoo-v9", 10)).toBe("zoo-v10");
  });

  it("appends a suffix to an unversioned id", () => {
    expect(nextGddId("handpicked-id", 4)).toBe("handpicked-id-v4");
  });
});

describe("editedGddDoc", () => {
  it("mints the successor: bumped version, supersedes, session provenance", () => {
    const old = doc();
    const next = editedGddDoc(old, replaceGddSection(BODY, 1, "Sharper. [OPEN]"));
    expect(next.version).toBe(4);
    expect(next.id).toBe("flagrush-v4");
    expect(next.supersedes).toBe(old.id);
    expect(next.source).toBe("session");
    expect(next.title).toBe(old.title);
    expect(next.hypotheses).toBe(old.hypotheses);
  });

  it("recounts honesty from the edited body", () => {
    const old = doc();
    expect(old.honesty.totals.open).toBe(1);
    const next = editedGddDoc(old, replaceGddSection(BODY, 0, "Done."));
    expect(next.honesty.totals.open).toBe(0);
    expect(next.honesty.totals.tbd).toBe(old.honesty.totals.tbd);
  });
});
