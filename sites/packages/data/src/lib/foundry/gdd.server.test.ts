import { describe, expect, it, vi } from "vitest";

import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import type { Pool } from "pg";

import {
  briefsQuotingAsk,
  changedSections,
  getGddDoc,
  listGroundBriefs,
  parseHonesty,
  readGddFile,
  splitGddSections,
  upsertGddDoc,
  parseAppendixHypotheses,
  parseGddText,
} from "./gdd.server";

describe("parseHonesty", () => {
  it("counts plain and arrowed markers alike — the arrow is a cross-reference, not a different marker", () => {
    const body = [
      "Preamble legend: [OPEN] means unreached — never counted.",
      "",
      "## Core loop",
      "[OPEN] not interviewed yet.",
      "[OPEN → §4] deferred to the systems section.",
      "[HYPOTHESIS] players will trade.",
      "[HYPOTHESIS → H1-02] retention holds a week.",
      "TBD: exact respawn timing.",
      "[agent-decided] four teams, not two.",
    ].join("\n");
    const { sections, totals } = parseHonesty(body);
    expect(sections).toHaveLength(1);
    expect(sections[0]).toEqual({
      name: "Core loop",
      open: 2,
      tbd: 1,
      hypothesis: 2,
      agentDecided: 1,
    });
    expect(totals).toEqual({ open: 2, tbd: 1, hypothesis: 2, agentDecided: 1 });
  });

  it("does not read [OPENING]-style words as markers", () => {
    const body = ["## Doors", "[OPENING] cutscene plays first.", "The [OPENER] speaks."].join(
      "\n",
    );
    const { totals } = parseHonesty(body);
    expect(totals.open).toBe(0);
  });
});

describe("readGddFile sources", () => {
  it("honors a frontmatter source of program", () => {
    const dir = mkdtempSync(join(tmpdir(), "gdd-src-"));
    const file = join(dir, "program-doc.md");
    writeFileSync(
      file,
      ["---", "id: program-doc", "kind: brief", "source: program", "---", "Body."].join(
        "\n",
      ),
    );
    expect(readGddFile(file).source).toBe("program");
  });

  it("throws on an unrecognized frontmatter source instead of relabeling it", () => {
    const dir = mkdtempSync(join(tmpdir(), "gdd-src-"));
    const file = join(dir, "mystery-doc.md");
    writeFileSync(
      file,
      ["---", "id: mystery-doc", "source: oracle", "---", "Body."].join("\n"),
    );
    expect(() => readGddFile(file)).toThrow(/refusing to relabel/);
    expect(() => readGddFile(file)).toThrow(/"oracle"/);
  });

  it("parses the vendored open-ground brief: program-drafted, no game named", () => {
    const fixture = fileURLToPath(
      new URL("../../fixtures/gdd/open-ground-game-clubs-v1.md", import.meta.url),
    );
    const doc = readGddFile(fixture);
    expect(doc.kind).toBe("brief");
    expect(doc.source).toBe("program");
    expect(doc.sceneId).toBeNull();
    expect(doc.hypotheses).toHaveLength(3);
    expect(doc.hypotheses.every((h) => h.status === "parked")).toBe(true);
    expect(doc.honesty.totals).toEqual({
      open: 1,
      tbd: 1,
      hypothesis: 3,
      agentDecided: 1,
    });
    expect(doc.honesty.sections).toHaveLength(4);
    // The grounding travels as stored keys, never parsed prose.
    expect(doc.groundsCell).toBe("community-operated-game-clubs");
    expect(doc.groundingRequestIds).toEqual([
      "ask-924f331f1bede6dc",
      "ask-c6d135026d20339c",
    ]);
  });

  it("reads absent grounding keys as null and empty — no invented ground", () => {
    const dir = mkdtempSync(join(tmpdir(), "gdd-src-"));
    const file = join(dir, "ungrounded-doc.md");
    writeFileSync(
      file,
      ["---", "id: ungrounded-doc", "kind: brief", "source: program", "---", "Body."].join(
        "\n",
      ),
    );
    const doc = readGddFile(file);
    expect(doc.groundsCell).toBeNull();
    expect(doc.groundingRequestIds).toEqual([]);
  });
});

// Enough of Postgres to exercise the grounding columns in memory: the upsert
// stores its parameter row keyed by id (the real table's PK), the readers
// answer from that store, and jsonb containment is modelled as array
// membership on the stored grounding ids — the same question `@>` answers.
function stubGddDb() {
  const stored = new Map<string, unknown[]>();
  const db = {
    query: vi.fn(async (sql: string, values: unknown[] = []) => {
      if (/INSERT INTO foundry\.gdd_doc/.test(sql)) {
        stored.set(values[0] as string, values);
        return { rowCount: 1, rows: [] };
      }
      const toRow = (v: unknown[]) => ({
        id: v[0],
        title: v[1],
        kind: v[2],
        scene_id: v[3],
        version: v[4],
        supersedes: v[5],
        source: v[6],
        source_ref: v[7],
        body_md: v[8],
        honesty: JSON.parse(v[9] as string),
        hypotheses: JSON.parse(v[10] as string),
        grounds_cell: v[11],
        grounding_request_ids: JSON.parse(v[12] as string),
        created_at: v[13],
        updated_at: v[13],
      });
      if (/WHERE d\.id = \$1/.test(sql)) {
        const v = stored.get(values[0] as string);
        return { rowCount: v ? 1 : 0, rows: v ? [toRow(v)] : [] };
      }
      if (/grounding_request_ids @> to_jsonb\(\$1::text\)/.test(sql)) {
        const rows = [...stored.values()]
          .filter((v) =>
            (JSON.parse(v[12] as string) as string[]).includes(values[0] as string),
          )
          .map((v) => ({ id: v[0], kind: v[2] }));
        return { rowCount: rows.length, rows };
      }
      if (/grounds_cell IS NOT NULL/.test(sql)) {
        const rows = [...stored.values()]
          .filter((v) => v[6] === "program" && v[11] != null)
          .map((v) => ({ id: v[0], cell: v[11] }));
        return { rowCount: rows.length, rows };
      }
      throw new Error(`stubGddDb: unmodelled query — ${sql}`);
    }),
  };
  return db as unknown as Pool;
}

describe("gdd grounding keys", () => {
  const fixture = fileURLToPath(
    new URL("../../fixtures/gdd/open-ground-game-clubs-v1.md", import.meta.url),
  );

  it("round-trips grounds_cell and grounding_request_ids through the upsert", async () => {
    const db = stubGddDb();
    await upsertGddDoc(db, readGddFile(fixture));
    const doc = await getGddDoc(db, "open-ground-game-clubs-v1");
    expect(doc?.groundsCell).toBe("community-operated-game-clubs");
    expect(doc?.groundingRequestIds).toEqual([
      "ask-924f331f1bede6dc",
      "ask-c6d135026d20339c",
    ]);
  });

  it("lists the brief under its cell, and only briefs that declare one", async () => {
    const db = stubGddDb();
    await upsertGddDoc(db, readGddFile(fixture));
    const dir = mkdtempSync(join(tmpdir(), "gdd-ground-"));
    const bare = join(dir, "bare-doc.md");
    writeFileSync(
      bare,
      ["---", "id: bare-doc", "kind: brief", "source: program", "---", "Body."].join("\n"),
    );
    await upsertGddDoc(db, readGddFile(bare));
    expect(await listGroundBriefs(db)).toEqual([
      { id: "open-ground-game-clubs-v1", cell: "community-operated-game-clubs" },
    ]);
  });

  it("answers briefsQuotingAsk by stored key — a grounded ask matches, any other id does not", async () => {
    const db = stubGddDb();
    await upsertGddDoc(db, readGddFile(fixture));
    expect(await briefsQuotingAsk(db, "ask-924f331f1bede6dc")).toEqual([
      { id: "open-ground-game-clubs-v1", kind: "brief" },
    ]);
    expect(await briefsQuotingAsk(db, "ask-c6d135026d20339c")).toEqual([
      { id: "open-ground-game-clubs-v1", kind: "brief" },
    ]);
    expect(await briefsQuotingAsk(db, "ask-0000000000000000")).toEqual([]);
  });
});

describe("splitGddSections", () => {
  it("aligns index-for-index with parseHonesty and ships each section verbatim", () => {
    const body = [
      "Preamble the marker grid never counts.",
      "",
      "## Core loop",
      "[OPEN] not interviewed yet.",
      "TBD: exact respawn timing.",
      "",
      "## Systems",
      "The flag rolls to one of three bases.",
    ].join("\n");
    const honesty = parseHonesty(body).sections;
    const sections = splitGddSections(body);
    expect(sections.map((s) => s.name)).toEqual(honesty.map((s) => s.name));
    // The markers stay in the text exactly where the author left them.
    expect(sections[0].contentMd).toContain("[OPEN] not interviewed yet.");
    expect(sections[0].contentMd).toContain("TBD: exact respawn timing.");
    expect(sections[1].contentMd).toContain("The flag rolls to one of three bases.");
    // The preamble belongs to the full document, not to any section.
    expect(sections.some((s) => s.contentMd.includes("Preamble"))).toBe(false);
  });
});

describe("parseAppendixHypotheses", () => {
  const body = [
    "## 3. Core Loop",
    "Rounds run on five-minute boundaries.",
    "",
    "## Appendix A — Hypothesis Log",
    "| IF/THEN | Section | Cheapest Test | Status | Location |",
    "| :--- | :--- | :--- | :--- | :--- |",
    "| IF players earn ability unlocks THEN retention increases | 4 | Arithmetic: Compare capture rates vs. unlock tiers | `parked` | TBD |",
    "| a row whose status is not a status | 2 | n/a | `someday` | TBD |",
  ].join("\n");

  it("parses the copilot's parked rows and skips non-status rows", () => {
    const rows = parseAppendixHypotheses(body);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      stage: "section 4",
      status: "parked",
      ifThen: "IF players earn ability unlocks THEN retention increases",
      test: "Arithmetic: Compare capture rates vs. unlock tiers",
    });
  });

  it("returns [] when there is no Appendix A", () => {
    expect(parseAppendixHypotheses("## 1. The Hook\nNothing else.")).toEqual([]);
  });

  // The observed hand-kept layout that positional parsing silently erased —
  // a leading ID column and a renamed test column pushed "Cheapest killing
  // test" into the status slot, so every row failed the status check and the
  // doc claimed "no hypotheses stored" over a recorded survived verdict.
  it("maps columns by header name: the seven-column hand-kept log parses", () => {
    const handKept = [
      "## Appendix A — Hypothesis Log",
      "",
      "| ID | IF / THEN (falsifiable) | Source section | Cheapest killing test | Status | Verdict / date | Tested on |",
      "|---|---|---|---|---|---|---|",
      "| H1-01 | IF the core cycle runs as placeholder text THEN a first-time player continues ≥5 cycles | §3 Core Loop | Paper/text prototype | survived | kill-check held · 2026-08-12 | paper (chat) |",
      "| H2-01 | IF card outcomes are deterministic THEN a third run is still completed | §3 Core Loop | Paper prototype, 3 runs | parked | | — |",
      "| H1-02 | IF the same consequence is staged vs text-only THEN staged reads as loop | §3 Core Loop | Greybox, owner self-test | active | | — |",
    ].join("\n");
    const rows = parseAppendixHypotheses(handKept);
    expect(rows).toHaveLength(3);
    expect(rows[0]).toMatchObject({
      id: "H1-01",
      status: "survived",
      stage: "section §3 Core Loop",
      test: "Paper/text prototype",
    });
    expect(rows[0].ifThen).toContain("first-time player continues");
    expect(rows.map((r) => r.status)).toEqual(["survived", "parked", "active"]);
  });

  it("falls back to the canonical positions when no header is recognizable", () => {
    const bare = [
      "## Appendix A — Hypothesis Log",
      "| A | B | C | D |",
      "|---|---|---|---|",
      "| IF x THEN y | 4 | cheap test | `active` |",
    ].join("\n");
    const rows = parseAppendixHypotheses(bare);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ status: "active", stage: "section 4" });
  });

  it("feeds parseGddText for published docs with no hypotheses dir", () => {
    const doc = parseGddText("---\ntitle: T\n---\n" + body, { source: "session" });
    expect(doc.hypotheses).toHaveLength(1);
    expect(doc.hypotheses[0].status).toBe("parked");
  });
});

describe("changedSections", () => {
  const v1 = "## 1. Hook\nold hook\n## 2. Loop\nsame\n## 3. Gone\nbye\n";
  const v2 = "## 1. Hook\nnew hook\n## 2. Loop\nsame\n## 4. Fresh\nhi\n";

  it("names edited, new, and removed sections in the newer body's order", () => {
    expect(changedSections(v2, v1)).toEqual([
      "1. Hook",
      "4. Fresh (new)",
      "3. Gone (removed)",
    ]);
  });

  it("returns [] for identical bodies — a real no-text-change reading", () => {
    expect(changedSections(v1, v1)).toEqual([]);
  });
});
