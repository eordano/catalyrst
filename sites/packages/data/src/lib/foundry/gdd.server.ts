import { readFileSync, readdirSync, statSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";

import matter from "gray-matter";
import type { Pool } from "pg";

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

const MARKERS = {
  open: /\[OPEN\]/g,
  tbd: /TBD:/g,
  hypothesis: /\[HYPOTHESIS\]/g,
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
  } catch {
    return [];
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
export function readGddFile(path: string, source: GddSource = "slack-import"): ParsedGddDoc {
  const file = resolve(path);
  const raw = readFileSync(file, "utf8");
  const { data, content } = matter(raw);
  const fm = data as Frontmatter;

  const title = fm.title?.trim() || basename(file, ".md");
  const version = Number(fm.version ?? 1) || 1;
  const kind = KINDS.includes(fm.kind as GddKind) ? (fm.kind as GddKind) : "shortgdd";
  const declared = fm.source === "copilot" || fm.source === "slack-import" ? fm.source : source;
  const bodyMd = content.replace(/^\n+/, "");

  return {
    id: fm.id?.trim() || `${slugify(title)}-v${version}`,
    title,
    kind,
    sceneId: fm.scene_id?.trim() || null,
    version,
    supersedes: fm.supersedes?.trim() || null,
    source: declared,
    sourceRef: fm.source_ref?.trim() || null,
    bodyMd,
    honesty: parseHonesty(bodyMd),
    hypotheses: fm.hypotheses ? loadHypotheses(join(dirname(file), fm.hypotheses)) : [],
    createdAt: asDate(fm.created_at, new Date().toISOString()),
  };
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
  } catch {
    return [];
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

/* ----------------------------------------------------------------- write -- */

export async function upsertGddDoc(db: Pool, doc: ParsedGddDoc): Promise<void> {
  await db.query(
    `INSERT INTO foundry.gdd_doc
       (id, title, kind, scene_id, version, supersedes, source, source_ref,
        body_md, honesty, hypotheses, created_at, updated_at)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11::jsonb,$12::timestamptz, now())
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
      doc.createdAt,
    ],
  );
}

/* ------------------------------------------------------------------ read -- */

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
  createdAt: string;
  updatedAt: string;
}

/** The list carries totals only: the per-section grid is the document's page. */
export async function listGddDocs(db: Pool): Promise<GddListRow[]> {
  const res = await db.query<DocDbRow & { scene_title: string | null }>(
    `SELECT d.id, d.title, d.kind, d.scene_id, d.version, d.supersedes, d.source,
            d.source_ref, '' AS body_md, d.honesty, d.hypotheses, d.created_at,
            d.updated_at, s.title AS scene_title
       FROM foundry.gdd_doc d
       LEFT JOIN foundry.scene s ON s.id = d.scene_id
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
      sceneId: doc.sceneId,
      sceneTitle: r.scene_title,
      source: doc.source,
      sourceRef: doc.sourceRef,
      honesty: doc.honesty.totals,
      hypothesisCounts: counts,
      createdAt: doc.createdAt,
      updatedAt: doc.updatedAt,
    };
  });
}

export async function getGddDoc(db: Pool, id: string): Promise<GddDoc | null> {
  const res = await db.query<DocDbRow>(
    `SELECT id, title, kind, scene_id, version, supersedes, source, source_ref,
            body_md, honesty, hypotheses, created_at, updated_at
       FROM foundry.gdd_doc WHERE id = $1`,
    [id],
  );
  const row = res.rows[0];
  return row ? toDoc(row) : null;
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
