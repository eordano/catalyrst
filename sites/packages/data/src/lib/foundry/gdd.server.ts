import { readFileSync, readdirSync, statSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";

import matter from "gray-matter";
import type { Pool, PoolClient } from "pg";

import type {
  GddDoc,
  GddHonesty,
  GddHonestySection,
  GddHypothesis,
  GddHypothesisStatus,
  GddKind,
  GddSource,
} from "./types";

// A shortGDD is honest about what it does not know: `TBD:` for an unknown with a
// plan to find out, `[HYPOTHESIS]` for a claim only a playtest can settle,
// `[agent-decided]` for a choice made on the owner's behalf, `[OPEN]` for a
// section the interview has not reached. Those four markers are the document's
// own coverage report, so the import counts them section by section instead of
// asking anyone to summarise the state of the doc in prose.
//
// Hypothesis statuses are never read out of the body. They live in the filename
// of each experiment file (`H<stage>-<nn>-<slug>_<status>.md`) — the skill's own
// state machine — and that is the only thing consulted here.

// [OPEN] and [HYPOTHESIS] also appear arrowed — "[OPEN → §4]",
// "[HYPOTHESIS → H1-02]" — so both match on the word boundary rather than the
// closing bracket; \b keeps [OPENING]-style words unmatched.
const MARKERS = {
  open: /\[OPEN\b/g,
  tbd: /TBD:/g,
  hypothesis: /\[HYPOTHESIS\b/g,
  agentDecided: /\[agent-decided\]/g,
} as const;

const STATUSES: readonly GddHypothesisStatus[] = [
  "parked",
  "active",
  "validated",
  "survived",
  "failed",
  "deferred",
];

const KINDS: readonly GddKind[] = ["shortgdd", "proposal", "brief", "feature-design"];

const GDD_SOURCES: readonly GddSource[] = [
  "slack-import",
  "copilot",
  "program",
  "session",
];

function count(text: string, re: RegExp): number {
  return text.match(new RegExp(re.source, "g"))?.length ?? 0;
}

/**
 * Splits the body on its `## ` headings and counts the honesty markers inside
 * each one.
 *
 * Text before the first heading is the document's own preamble — the adaptation
 * note that *explains* the markers — so counting it would score the legend as
 * unfinished work.
 */
export function parseHonesty(bodyMd: string): GddHonesty {
  const lines = bodyMd.split("\n");
  const sections: GddHonestySection[] = [];
  let current: { name: string; lines: string[] } | null = null;

  const close = () => {
    if (!current) return;
    const text = current.lines.join("\n");
    sections.push({
      name: current.name,
      open: count(text, MARKERS.open),
      tbd: count(text, MARKERS.tbd),
      hypothesis: count(text, MARKERS.hypothesis),
      agentDecided: count(text, MARKERS.agentDecided),
    });
  };

  for (const line of lines) {
    const heading = /^##\s+(.*\S)\s*$/.exec(line);
    if (heading) {
      close();
      current = { name: heading[1], lines: [] };
      continue;
    }
    current?.lines.push(line);
  }
  close();

  const totals = { open: 0, tbd: 0, hypothesis: 0, agentDecided: 0 };
  for (const s of sections) {
    totals.open += s.open;
    totals.tbd += s.tbd;
    totals.hypothesis += s.hypothesis;
    totals.agentDecided += s.agentDecided;
  }
  return { sections, totals };
}

export interface GddSectionContent {
  name: string;
  contentMd: string;
}

/** The same `## ` split parseHonesty counts by, but shipping each section's
 *  verbatim content — the arrays align index-for-index, so the marker grid's
 *  rows can expand to exactly the text their counts were counted in. */
export function splitGddSections(bodyMd: string): GddSectionContent[] {
  const lines = bodyMd.split("\n");
  const sections: GddSectionContent[] = [];
  let current: { name: string; lines: string[] } | null = null;

  const close = () => {
    if (!current) return;
    sections.push({ name: current.name, contentMd: current.lines.join("\n") });
  };

  for (const line of lines) {
    const heading = /^##\s+(.*\S)\s*$/.exec(line);
    if (heading) {
      close();
      current = { name: heading[1], lines: [] };
      continue;
    }
    current?.lines.push(line);
  }
  close();
  return sections;
}

/** Splices new content into section `index` of the body, leaving every other
 *  byte — preamble, heading lines as written, sibling sections — untouched, so
 *  an edit to one section can never reflow the rest of the document. */
export function replaceGddSection(
  bodyMd: string,
  index: number,
  contentMd: string,
): string {
  const lines = bodyMd.split("\n");
  let section = -1;
  let start = -1;
  let end = lines.length;
  for (let i = 0; i < lines.length; i++) {
    if (!/^##\s+(.*\S)\s*$/.test(lines[i])) continue;
    section += 1;
    if (section === index) {
      start = i + 1;
    } else if (section === index + 1) {
      end = i;
      break;
    }
  }
  if (start === -1) {
    throw new Error(`no section ${index} in this document to replace`);
  }
  const content = contentMd.replace(/\r\n/g, "\n").split("\n");
  return [...lines.slice(0, start), ...content, ...lines.slice(end)].join("\n");
}

const FILENAME = /^(H(\d+)-\d+)-(.+)_([a-z]+)\.md$/;

function bullet(text: string, label: string): string | undefined {
  const re = new RegExp(`^-\\s+\\*\\*${label}:?\\*\\*\\s*(.+)$`, "m");
  const value = re.exec(text)?.[1]?.trim();
  if (!value || value === "—" || value === "-") return undefined;
  return value;
}

/**
 * A mobile-sensitive claim tested only on desktop has not been tested. The
 * format records the two facts as separate bullets and the log has one column
 * for them, so the gap rides along in it instead of being dropped.
 */
function withMobileGap(testedOn: string, mobileSensitive: string | undefined): string {
  if (!/^yes\b/i.test(mobileSensitive ?? "")) return testedOn;
  if (!/desktop/i.test(testedOn) || /mobile/i.test(testedOn)) return testedOn;
  return `${testedOn} — mobile pending`;
}

/**
 * Reads one experiment file. The status is the filename suffix; the body only
 * supplies the claim and the test it would take to kill it, and a field the
 * author left as an em dash stays absent rather than becoming an empty string.
 */
export function parseHypothesisFile(path: string, stage: string): GddHypothesis | null {
  const m = FILENAME.exec(basename(path));
  if (!m) return null;
  const status = m[4] as GddHypothesisStatus;
  if (!STATUSES.includes(status)) return null;

  const text = readFileSync(path, "utf8");
  const hypothesis: GddHypothesis = {
    id: m[1],
    stage,
    slug: m[3],
    status,
  };
  const ifThen = bullet(text, "IF/THEN");
  const test = bullet(text, "Cheapest killing test");
  const testedOn = bullet(text, "Tested on");
  if (ifThen) hypothesis.ifThen = ifThen;
  if (test) hypothesis.test = test;
  if (testedOn) hypothesis.testedOn = withMobileGap(testedOn, bullet(text, "Mobile-sensitive"));
  return hypothesis;
}

/** Every `<stage>/H*_<status>.md` under `dir`, stage-ordered then file-ordered. */
export function loadHypotheses(dir: string): GddHypothesis[] {
  let stages: string[];
  try {
    stages = readdirSync(dir, { withFileTypes: true })
      .filter((e) => e.isDirectory())
      .map((e) => e.name)
      .sort();
  } catch (err) {
    // ENOENT: this document has no stage directories, so it has no hypotheses —
    // a real empty reading. Any other fault rethrows instead of being hidden.
    if ((err as NodeJS.ErrnoException).code === "ENOENT") return [];
    throw err;
  }
  const out: GddHypothesis[] = [];
  for (const stage of stages) {
    const files = readdirSync(join(dir, stage))
      .filter((f) => f.endsWith(".md"))
      .sort();
    for (const file of files) {
      const parsed = parseHypothesisFile(join(dir, stage, file), stage);
      if (parsed) out.push(parsed);
    }
  }
  return out;
}

export interface ParsedGddDoc extends Omit<GddDoc, "updatedAt"> {}

interface Frontmatter {
  id?: string;
  title?: string;
  kind?: string;
  version?: number;
  supersedes?: string;
  scene_id?: string;
  source?: string;
  source_ref?: string;
  created_at?: string;
  hypotheses?: string;
  grounds_cell?: string;
  grounding_asks?: unknown;
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

function asDate(value: unknown, fallback: string): string {
  if (value instanceof Date) return value.toISOString();
  const parsed = Date.parse(String(value ?? ""));
  return Number.isNaN(parsed) ? fallback : new Date(parsed).toISOString();
}

/**
 * Reads a vendored fixture into the row shape.
 *
 * `hypotheses:` names a sibling directory of experiment files; a document that
 * shipped without one imports with an empty log, because a hypothesis nobody
 * filed is not a hypothesis this site can show.
 */
export interface ParseGddTextOptions {
  source?: GddSource;
  /** Used when the frontmatter names no source_ref of its own. */
  sourceRef?: string | null;
  /** Used when the frontmatter names no title; absent both ways is an error. */
  fallbackTitle?: string;
  /** How `hypotheses:` in the frontmatter resolves; file readers pass the
   *  sibling-directory loader, transcript readers have no directory to offer. */
  hypothesesLoader?: (name: string) => ParsedGddDoc["hypotheses"];
  /** Names the input in error messages. */
  label?: string;
}


/**
 * The Appendix A fallback: a published doc (copilot/session) has no stage
 * directory, but the copilot's own /gdd command parks falsifiable hypotheses
 * in an "Appendix A — Hypothesis Log" table
 * (| IF/THEN | Section | Cheapest Test | Status | Location |). Parse those
 * rows so the park→test→verdict loop can track exactly the docs the copilot
 * produces. File-backed docs keep the filename state machine as their truth
 * and never reach this parser.
 */
export function parseAppendixHypotheses(bodyMd: string): GddHypothesis[] {
  // String ops, not a lazy regex: with the m flag `$` matches every line end
  // and silently truncates the captured section to its first line.
  const start = bodyMd.search(/^##\s+Appendix A/m);
  if (start === -1) return [];
  const after = bodyMd.slice(start);
  const nextIdx = after.indexOf("\n## ", 3);
  const section = nextIdx === -1 ? after : after.slice(0, nextIdx);
  const tableLines = section.split("\n").filter((l) => l.trim().startsWith("|"));
  if (tableLines.length < 3) return [];

  // Columns are found by header NAME, positional only as the fallback: the
  // copilot's canonical layout is | IF/THEN | Section | Cheapest Test |
  // Status | Location |, but hand-kept logs reorder and prepend columns — an
  // observed seven-column | ID | IF/THEN | Source section | Cheapest killing
  // test | Status | Verdict/date | Tested on | lost every row under
  // positional parsing, including the program's only recorded verdict.
  const headers = tableLines[0]
    .split("|")
    .slice(1, -1)
    .map((c) => c.trim().toLowerCase());
  const col = (re: RegExp, fallback: number): number => {
    const i = headers.findIndex((h) => re.test(h));
    return i === -1 ? fallback : i;
  };
  const idCol = headers.findIndex((h) => /^id\b/.test(h));
  const ifThenCol = col(/^if\b.*then/, 0);
  const sectionCol = col(/section/, 1);
  const testCol = col(/test/, 2);
  const statusCol = col(/status/, 3);

  const out: GddHypothesis[] = [];
  for (const [i, line] of tableLines.slice(2).entries()) {
    const cells = line
      .split("|")
      .slice(1, -1)
      .map((c) => c.trim());
    if (cells.length <= Math.max(ifThenCol, statusCol)) continue;
    const ifThen = cells[ifThenCol];
    const sectionCell = cells[sectionCol] ?? "";
    const test = cells[testCol] ?? "";
    const status = (cells[statusCol] ?? "").replace(/`/g, "").trim().toLowerCase();
    if (!(STATUSES as readonly string[]).includes(status)) continue;
    if (!ifThen) continue;
    const rowId = idCol === -1 ? "" : (cells[idCol] ?? "");
    out.push({
      id: rowId || `appendix-${i + 1}`,
      stage: sectionCell ? `section ${sectionCell}` : "appendix",
      slug: slugify(ifThen).slice(0, 64),
      status: status as GddHypothesisStatus,
      ifThen,
      test: test || undefined,
    });
  }
  return out;
}


/** Which sections differ between two versions of a doc — names in the NEWER
 *  body's order, plus removals. Derived from the stored bodies on read, never
 *  authored or cached, so it can never drift from what the versions say. */
export function changedSections(newerBody: string, olderBody: string): string[] {
  const newer = splitGddSections(newerBody)
  const older = splitGddSections(olderBody)
  const olderByName = new Map(older.map((s) => [s.name, s.contentMd.trim()]))
  const out: string[] = []
  const seen = new Set<string>()
  for (const s of newer) {
    seen.add(s.name)
    const prev = olderByName.get(s.name)
    if (prev === undefined) out.push(`${s.name} (new)`)
    else if (prev !== s.contentMd.trim()) out.push(s.name)
  }
  for (const s of older) {
    if (!seen.has(s.name)) out.push(`${s.name} (removed)`)
  }
  return out
}

/** The row shape from raw markdown, wherever the markdown came from. */
export function parseGddText(
  raw: string,
  opts: ParseGddTextOptions = {},
): ParsedGddDoc {
  const label = opts.label ?? "document";
  const { data, content } = matter(raw);
  const fm = data as Frontmatter;

  const title = fm.title?.trim() || opts.fallbackTitle;
  if (!title) {
    throw new Error(
      `${label}: no \`title:\` in the frontmatter and no fallback to name it`,
    );
  }
  const version = Number(fm.version ?? 1) || 1;
  const kind = KINDS.includes(fm.kind as GddKind) ? (fm.kind as GddKind) : "shortgdd";
  if (fm.source !== undefined && !GDD_SOURCES.includes(fm.source as GddSource)) {
    throw new Error(
      `${label}: frontmatter source "${fm.source}" is not one of ` +
        `${GDD_SOURCES.join(", ")} — refusing to relabel it`,
    );
  }
  const declared = (fm.source as GddSource | undefined) ?? opts.source ?? "slack-import";
  const bodyMd = content.replace(/^\n+/, "");

  return {
    id: fm.id?.trim() || `${slugify(title)}-v${version}`,
    title,
    kind,
    sceneId: fm.scene_id?.trim() || null,
    version,
    supersedes: fm.supersedes?.trim() || null,
    source: declared,
    sourceRef: fm.source_ref?.trim() || opts.sourceRef || null,
    bodyMd,
    honesty: parseHonesty(bodyMd),
    hypotheses:
      fm.hypotheses && opts.hypothesesLoader
        ? opts.hypothesesLoader(fm.hypotheses)
        : parseAppendixHypotheses(bodyMd),
    groundsCell: fm.grounds_cell?.trim() || null,
    groundingRequestIds: Array.isArray(fm.grounding_asks)
      ? fm.grounding_asks.map(String)
      : [],
    createdAt: asDate(fm.created_at, new Date().toISOString()),
  };
}

export function readGddFile(path: string, source: GddSource = "slack-import"): ParsedGddDoc {
  const file = resolve(path);
  return parseGddText(readFileSync(file, "utf8"), {
    source,
    fallbackTitle: basename(file, ".md"),
    label: basename(file),
    hypothesesLoader: (name) => loadHypotheses(join(dirname(file), name)),
  });
}

/**
 * Scans a copilot workspace for the shortGDDs it has drafted.
 *
 * The skills write `<project>/design/shortGDD.md` with the experiment files in
 * stage folders beside it, so the document's own directory is its hypothesis
 * directory. `source_ref` is the workspace-relative path — where the file
 * actually is, not a story about who asked for it.
 */
export function scanWorkspace(workspace: string): ParsedGddDoc[] {
  const root = resolve(workspace);
  const projects = join(root, "projects");
  let entries: string[];
  try {
    entries = readdirSync(projects, { withFileTypes: true })
      .filter((e) => e.isDirectory())
      .map((e) => e.name)
      .sort();
  } catch (err) {
    // ENOENT is the filesystem stating this workspace has no projects/ directory
    // yet — that is a real empty scan, not a hidden failure. Anything else (a
    // permissions fault, an I/O error) rethrows rather than arriving as "no docs".
    if ((err as NodeJS.ErrnoException).code === "ENOENT") return [];
    throw err;
  }

  const out: ParsedGddDoc[] = [];
  for (const project of entries) {
    const design = join(projects, project, "design");
    const doc = join(design, "shortGDD.md");
    try {
      statSync(doc);
    } catch {
      continue;
    }
    const parsed = readGddFile(doc, "copilot");
    out.push({
      ...parsed,
      id: parsed.id === `-v${parsed.version}` ? `${project}-v${parsed.version}` : parsed.id,
      source: "copilot",
      sourceRef: `projects/${project}/design/shortGDD.md`,
      hypotheses: parsed.hypotheses.length > 0 ? parsed.hypotheses : loadHypotheses(design),
    });
  }
  return out;
}

export async function upsertGddDoc(
  db: Pool | PoolClient,
  doc: ParsedGddDoc,
): Promise<void> {
  await db.query(
    `INSERT INTO foundry.gdd_doc
       (id, title, kind, scene_id, version, supersedes, source, source_ref,
        body_md, honesty, hypotheses, grounds_cell, grounding_request_ids,
        created_at, updated_at)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11::jsonb,$12,$13::jsonb,
             $14::timestamptz, now())
     ON CONFLICT (id) DO UPDATE SET
       title      = EXCLUDED.title,
       kind       = EXCLUDED.kind,
       scene_id   = EXCLUDED.scene_id,
       version    = EXCLUDED.version,
       supersedes = EXCLUDED.supersedes,
       source     = EXCLUDED.source,
       source_ref = EXCLUDED.source_ref,
       body_md    = EXCLUDED.body_md,
       honesty    = EXCLUDED.honesty,
       hypotheses = EXCLUDED.hypotheses,
       grounds_cell = EXCLUDED.grounds_cell,
       grounding_request_ids = EXCLUDED.grounding_request_ids,
       created_at = EXCLUDED.created_at,
       updated_at = now()`,
    [
      doc.id,
      doc.title,
      doc.kind,
      doc.sceneId,
      doc.version,
      doc.supersedes,
      doc.source,
      doc.sourceRef,
      doc.bodyMd,
      JSON.stringify(doc.honesty),
      JSON.stringify(doc.hypotheses),
      doc.groundsCell,
      JSON.stringify(doc.groundingRequestIds),
      doc.createdAt,
    ],
  );
}

type DocDbRow = {
  id: string;
  title: string;
  kind: string;
  scene_id: string | null;
  version: number;
  supersedes: string | null;
  source: string;
  source_ref: string | null;
  body_md: string;
  honesty: unknown;
  hypotheses: unknown;
  grounds_cell?: string | null;
  grounding_request_ids?: unknown;
  created_at: Date | string;
  updated_at: Date | string;
};

const EMPTY_HONESTY: GddHonesty = {
  sections: [],
  totals: { open: 0, tbd: 0, hypothesis: 0, agentDecided: 0 },
};

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : String(value);
}

function toHonesty(value: unknown): GddHonesty {
  if (!value || typeof value !== "object") return EMPTY_HONESTY;
  const v = value as Partial<GddHonesty>;
  return {
    sections: Array.isArray(v.sections) ? v.sections : [],
    totals: v.totals ?? EMPTY_HONESTY.totals,
  };
}

function toDoc(r: DocDbRow): GddDoc {
  return {
    id: r.id,
    title: r.title,
    kind: r.kind as GddKind,
    sceneId: r.scene_id,
    version: Number(r.version),
    supersedes: r.supersedes,
    source: r.source as GddSource,
    sourceRef: r.source_ref,
    bodyMd: r.body_md,
    honesty: toHonesty(r.honesty),
    hypotheses: Array.isArray(r.hypotheses) ? (r.hypotheses as GddHypothesis[]) : [],
    groundsCell: r.grounds_cell ?? null,
    groundingRequestIds: Array.isArray(r.grounding_request_ids)
      ? r.grounding_request_ids.map(String)
      : [],
    createdAt: iso(r.created_at),
    updatedAt: iso(r.updated_at),
  };
}

export interface GddListRow {
  id: string;
  title: string;
  kind: GddKind;
  version: number;
  /** The doc this one replaces, if any — so the list can flag the older one. */
  supersedes: string | null;
  sceneId: string | null;
  sceneTitle: string | null;
  source: GddSource;
  sourceRef: string | null;
  honesty: GddHonesty["totals"];
  hypothesisCounts: Partial<Record<GddHypothesisStatus, number>>;
  /** Stored signature count on this exact version; 0 = nobody signed it. */
  approvals: number;
  createdAt: string;
  updatedAt: string;
}

/** The list carries totals only: the per-section grid is the document's page.
 *  A doc's game resolves in either direction — the doc's own declared scene, or
 *  a scene whose operator-set gdd_doc_id names the doc — so "proposed — not
 *  built here" renders only when neither side links. */
export async function listGddDocs(db: Pool): Promise<GddListRow[]> {
  const res = await db.query<
    DocDbRow & {
      linked_scene_id: string | null;
      scene_title: string | null;
      approvals: number;
    }
  >(
    `SELECT d.id, d.title, d.kind, d.scene_id, d.version, d.supersedes, d.source,
            d.source_ref, '' AS body_md, d.honesty, d.hypotheses, d.created_at,
            d.updated_at, s.id AS linked_scene_id, s.title AS scene_title,
            (SELECT count(*)::int FROM foundry.gdd_approval ga
              WHERE ga.doc_id = d.id) AS approvals
       FROM foundry.gdd_doc d
       LEFT JOIN LATERAL (
         SELECT sc.id, sc.title FROM foundry.scene sc
          WHERE sc.id = d.scene_id OR sc.gdd_doc_id = d.id
          ORDER BY (sc.id = d.scene_id) DESC, sc.id
          LIMIT 1
       ) s ON true
      ORDER BY d.created_at, d.version, d.id`,
  );

  return res.rows.map((r) => {
    const doc = toDoc(r);
    const counts: Partial<Record<GddHypothesisStatus, number>> = {};
    for (const h of doc.hypotheses) counts[h.status] = (counts[h.status] ?? 0) + 1;
    return {
      id: doc.id,
      title: doc.title,
      kind: doc.kind,
      version: doc.version,
      supersedes: doc.supersedes,
      sceneId: r.linked_scene_id ?? doc.sceneId,
      sceneTitle: r.scene_title,
      source: doc.source,
      sourceRef: doc.sourceRef,
      honesty: doc.honesty.totals,
      hypothesisCounts: counts,
      approvals: Number(r.approvals),
      createdAt: doc.createdAt,
      updatedAt: doc.updatedAt,
    };
  });
}

/** scene_id resolves in either direction (see listGddDocs), so the doc page's
 *  "no game is built from it" note is true whichever side declared the link. */
export async function getGddDoc(db: Pool, id: string): Promise<GddDoc | null> {
  const res = await db.query<DocDbRow>(
    `SELECT d.id, d.title, d.kind,
            coalesce(d.scene_id, (SELECT sc.id FROM foundry.scene sc
              WHERE sc.gdd_doc_id = d.id ORDER BY sc.id LIMIT 1)) AS scene_id,
            d.version, d.supersedes, d.source, d.source_ref,
            d.body_md, d.honesty, d.hypotheses, d.grounds_cell,
            d.grounding_request_ids, d.created_at, d.updated_at
       FROM foundry.gdd_doc d WHERE d.id = $1`,
    [id],
  );
  const row = res.rows[0];
  return row ? toDoc(row) : null;
}

export interface GddChainRow {
  id: string;
  version: number;
  supersedes: string | null;
  source: GddSource;
  honesty: GddHonesty["totals"];
  createdAt: string;
}

/** Every version in this doc's supersedes chain, walked in both directions
 *  from `id`, version-ordered. Per-version stats are each row's own stored
 *  honesty totals, never recomputed. */
export async function getGddChain(db: Pool, id: string): Promise<GddChainRow[]> {
  const res = await db.query<{
    id: string;
    version: number;
    supersedes: string | null;
    source: string;
    honesty: unknown;
    created_at: Date | string;
  }>(
    `WITH RECURSIVE older AS (
       SELECT d.id, d.version, d.supersedes, d.source, d.honesty, d.created_at
         FROM foundry.gdd_doc d WHERE d.id = $1
       UNION
       SELECT d.id, d.version, d.supersedes, d.source, d.honesty, d.created_at
         FROM foundry.gdd_doc d JOIN older o ON o.supersedes = d.id
     ), newer AS (
       SELECT d.id, d.version, d.supersedes, d.source, d.honesty, d.created_at
         FROM foundry.gdd_doc d WHERE d.id = $1
       UNION
       SELECT d.id, d.version, d.supersedes, d.source, d.honesty, d.created_at
         FROM foundry.gdd_doc d JOIN newer n ON d.supersedes = n.id
     )
     SELECT * FROM older UNION SELECT * FROM newer
     ORDER BY version, id`,
    [id],
  );
  return res.rows.map((r) => ({
    id: r.id,
    version: Number(r.version),
    supersedes: r.supersedes,
    source: r.source as GddSource,
    honesty: toHonesty(r.honesty).totals,
    createdAt: iso(r.created_at),
  }));
}

/** The newest revision in `id`'s supersedes chain — the version people land
 *  on. Returns the doc itself when nothing supersedes it, null when the id is
 *  unknown. */
export async function getGddChainHead(
  db: Pool,
  id: string,
): Promise<{ id: string; version: number; title: string } | null> {
  const res = await db.query<{ id: string; version: number; title: string }>(
    `WITH RECURSIVE newer AS (
       SELECT d.id, d.version, d.title
         FROM foundry.gdd_doc d WHERE d.id = $1
       UNION
       SELECT d.id, d.version, d.title
         FROM foundry.gdd_doc d JOIN newer n ON d.supersedes = n.id
     )
     SELECT id, version, title FROM newer
      ORDER BY version DESC, id
      LIMIT 1`,
    [id],
  );
  const r = res.rows[0];
  return r ? { id: r.id, version: Number(r.version), title: r.title } : null;
}

export interface GddSceneReadingRow {
  gddDocId: string;
  sceneId: string;
  relation: "same-concept";
  rationale: string;
  confidence: "evidence-backed" | "inferred";
  basis: string;
  readAt: string;
}

export interface GddSceneReading extends GddSceneReadingRow {
  docTitle: string;
  /** Version of the doc the judgment was read against — supersede-on-edit
   *  mints new ids, so a reading can name an older version than the chain
   *  head a page links. */
  docVersion: number;
  sceneTitle: string;
}

const READING_COLUMNS = `gr.gdd_doc_id, gr.scene_id, gr.relation, gr.rationale,
       gr.confidence, gr.basis, gr.read_at,
       d.title AS doc_title, d.version AS doc_version, s.title AS scene_title`;

type ReadingDbRow = {
  gdd_doc_id: string;
  scene_id: string;
  relation: string;
  rationale: string;
  confidence: string;
  basis: string;
  read_at: Date | string;
  doc_title: string;
  doc_version: number;
  scene_title: string;
};

function toReading(r: ReadingDbRow): GddSceneReading {
  return {
    gddDocId: r.gdd_doc_id,
    sceneId: r.scene_id,
    relation: r.relation as "same-concept",
    rationale: r.rationale,
    confidence: r.confidence as "evidence-backed" | "inferred",
    basis: r.basis,
    readAt: iso(r.read_at),
    docTitle: r.doc_title,
    docVersion: Number(r.doc_version),
    sceneTitle: r.scene_title,
  };
}

/** This program's dated same-concept readings naming any of these docs —
 *  callers pass a whole supersedes chain so a reading made against one
 *  version keeps resolving after edits mint newer ids. */
export async function readingsForDocs(
  db: Pool,
  docIds: readonly string[],
): Promise<GddSceneReading[]> {
  if (docIds.length === 0) return [];
  const res = await db.query<ReadingDbRow>(
    `SELECT ${READING_COLUMNS}
       FROM foundry.gdd_scene_reading gr
       JOIN foundry.gdd_doc d ON d.id = gr.gdd_doc_id
       JOIN foundry.scene s ON s.id = gr.scene_id
      WHERE gr.gdd_doc_id = ANY($1)
      ORDER BY gr.read_at, gr.scene_id, gr.gdd_doc_id`,
    [docIds],
  );
  return res.rows.map(toReading);
}

/** This program's dated same-concept readings naming this scene. */
export async function readingForScene(
  db: Pool,
  sceneId: string,
): Promise<GddSceneReading[]> {
  const res = await db.query<ReadingDbRow>(
    `SELECT ${READING_COLUMNS}
       FROM foundry.gdd_scene_reading gr
       JOIN foundry.gdd_doc d ON d.id = gr.gdd_doc_id
       JOIN foundry.scene s ON s.id = gr.scene_id
      WHERE gr.scene_id = $1
      ORDER BY gr.read_at, gr.gdd_doc_id`,
    [sceneId],
  );
  return res.rows.map(toReading);
}

/**
 * Upserts same-concept readings keyed by (doc, scene). Content columns only: a
 * re-import refreshes the judgment in place and never duplicates. A row naming
 * a doc or scene the registry lacks is skipped with a warning rather than
 * landed dangling — a judgment lands whole or not at all.
 */
export async function upsertGddSceneReadings(
  db: Pool | PoolClient,
  rows: GddSceneReadingRow[],
): Promise<{ upserted: number; skipped: string[] }> {
  let upserted = 0;
  const skipped: string[] = [];
  for (const row of rows) {
    const key = `${row.gddDocId} ↔ ${row.sceneId}`;
    const doc = await db.query("SELECT 1 FROM foundry.gdd_doc WHERE id = $1", [
      row.gddDocId,
    ]);
    if ((doc.rowCount ?? 0) === 0) {
      console.warn(`gdd-scene-readings: no foundry.gdd_doc row for "${row.gddDocId}" — reading skipped`);
      skipped.push(key);
      continue;
    }
    const scene = await db.query("SELECT 1 FROM foundry.scene WHERE id = $1", [
      row.sceneId,
    ]);
    if ((scene.rowCount ?? 0) === 0) {
      console.warn(`gdd-scene-readings: no foundry.scene row for "${row.sceneId}" — reading skipped`);
      skipped.push(key);
      continue;
    }
    await db.query(
      `INSERT INTO foundry.gdd_scene_reading
         (gdd_doc_id, scene_id, relation, rationale, confidence, basis, read_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7::timestamptz)
       ON CONFLICT (gdd_doc_id, scene_id) DO UPDATE SET
         relation   = EXCLUDED.relation,
         rationale  = EXCLUDED.rationale,
         confidence = EXCLUDED.confidence,
         basis      = EXCLUDED.basis,
         read_at    = EXCLUDED.read_at`,
      [
        row.gddDocId,
        row.sceneId,
        row.relation,
        row.rationale,
        row.confidence,
        row.basis,
        row.readAt,
      ],
    );
    upserted += 1;
  }
  return { upserted, skipped };
}

/** The newer revision naming this doc in its `supersedes`, if one exists. */
export async function getSupersededBy(
  db: Pool,
  id: string,
): Promise<{ id: string; version: number } | null> {
  const res = await db.query<{ id: string; version: number }>(
    `SELECT id, version FROM foundry.gdd_doc
      WHERE supersedes = $1
      ORDER BY version DESC
      LIMIT 1`,
    [id],
  );
  const r = res.rows[0];
  return r ? { id: r.id, version: Number(r.version) } : null;
}

export interface GddDocSummary {
  id: string;
  title: string;
  kind: GddKind;
  version: number;
  open: number;
  hypotheses: number;
}

/** What a game page shows about its design doc without loading the body. */
export async function getGddDocSummary(
  db: Pool,
  id: string,
): Promise<GddDocSummary | null> {
  const res = await db.query<{
    id: string;
    title: string;
    kind: string;
    version: number;
    open: number;
    hypotheses: number;
  }>(
    `SELECT id, title, kind, version,
            coalesce((honesty -> 'totals' ->> 'open')::int, 0) AS open,
            coalesce(jsonb_array_length(hypotheses), 0) AS hypotheses
       FROM foundry.gdd_doc WHERE id = $1`,
    [id],
  );
  const r = res.rows[0];
  return r
    ? {
        id: r.id,
        title: r.title,
        kind: r.kind as GddKind,
        version: Number(r.version),
        open: Number(r.open),
        hypotheses: Number(r.hypotheses),
      }
    : null;
}

/** Program briefs that declare the market cell they read — stored keys, so the
 *  shelf links each cell to its brief without parsing prose. */
export async function listGroundBriefs(
  db: Pool,
): Promise<{ id: string; cell: string }[]> {
  const res = await db.query<{ id: string; cell: string }>(
    `SELECT id, grounds_cell AS cell FROM foundry.gdd_doc
      WHERE source = 'program' AND grounds_cell IS NOT NULL
      ORDER BY created_at, id`,
  );
  return res.rows.map((r) => ({ id: r.id, cell: r.cell }));
}

/** Titles for a doc's stored grounding keys, so the doc page can name the
 *  demand it answers. An id with no surviving ask row simply drops out —
 *  never an invented title. */
export async function asksByIds(
  db: Pool,
  ids: readonly string[],
): Promise<{ id: string; title: string }[]> {
  if (ids.length === 0) return [];
  const res = await db.query<{ id: string; title: string }>(
    `SELECT id, title FROM foundry.request WHERE id = ANY($1)
      ORDER BY created_at, id`,
    [[...ids]],
  );
  return res.rows;
}

/** The last editor of a session-source doc: the persona name behind the
 *  edit_gdd_doc row whose subject is this doc id, or null when the editor
 *  never claimed one. A raw sid never leaves this function. */
export async function docEditor(
  db: Pool,
  docId: string,
): Promise<string | null> {
  const res = await db.query<{ display_name: string | null }>(
    `SELECT p.display_name
       FROM foundry.action_log a
       LEFT JOIN foundry.sid_alias al ON al.alias_sid = a.sid
       LEFT JOIN foundry.persona p ON p.sid = COALESCE(al.persona_sid, a.sid)
      WHERE a.action = 'edit_gdd_doc' AND a.subject = $1
      ORDER BY a.at DESC LIMIT 1`,
    [docId],
  );
  return res.rows[0]?.display_name ?? null;
}

/** Docs whose stored grounding keys name this ask — jsonb containment on
 *  grounding_request_ids, never a text search of the body. Any source counts:
 *  the program's own ground briefs and copilot-session briefs published by a
 *  visitor both ground on demand the same way. */
export async function briefsQuotingAsk(
  db: Pool,
  requestId: string,
): Promise<{ id: string; kind: string }[]> {
  const res = await db.query<{ id: string; kind: string }>(
    `SELECT id, kind FROM foundry.gdd_doc
      WHERE grounding_request_ids @> to_jsonb($1::text)
      ORDER BY created_at, id`,
    [requestId],
  );
  return res.rows.map((r) => ({ id: r.id, kind: r.kind }));
}
