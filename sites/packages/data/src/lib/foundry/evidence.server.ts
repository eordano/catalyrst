import { promises as fs } from "node:fs";
import { join, resolve, sep } from "node:path";

import type { FoundryDb } from "./trajectory.server";

// A run's evidence is an operator-side directory recorded at ingest time. The
// absolute host path never leaves the server: pages carry the basename+hash
// label, and files are served only through the sanitized resource route below.

export type EvidenceRunKind = "trajectory" | "report";

export type EvidenceRun = {
  runId: string;
  kind: EvidenceRunKind;
  evidencePath: string | null;
  sceneId: string | null;
  createdAt: string;
};

/** Resolves a run id — a trajectory id or a bot-report id — to its recorded
 *  evidence path. Null when no such run exists. */
export async function resolveEvidenceRun(
  db: FoundryDb,
  runId: string,
): Promise<EvidenceRun | null> {
  const traj = await db.query<{
    id: string;
    evidence_path: string | null;
    scene_id: string | null;
    created_at: Date;
  }>(
    `SELECT id, evidence_path, scene_id, created_at
       FROM foundry.trajectory WHERE id = $1`,
    [runId],
  );
  const t = traj.rows[0];
  if (t) {
    return {
      runId: t.id,
      kind: "trajectory",
      evidencePath: t.evidence_path,
      sceneId: t.scene_id,
      createdAt: t.created_at.toISOString(),
    };
  }
  const rep = await db.query<{
    id: string;
    evidence_path: string | null;
    scene_id: string | null;
    ran_at: Date;
  }>(
    `SELECT id, evidence_path, scene_id, ran_at
       FROM foundry.bot_report WHERE id = $1`,
    [runId],
  );
  const r = rep.rows[0];
  if (!r) return null;
  return {
    runId: r.id,
    kind: "report",
    evidencePath: r.evidence_path,
    sceneId: r.scene_id,
    createdAt: r.ran_at.toISOString(),
  };
}

const SHOT_EXT = /\.(jpe?g|png|webp|gif)$/i;
const LOG_TAIL_LINES = 100;

export type EvidenceListing = {
  /** The directory still exists on the operator host. */
  present: boolean;
  /** Shot file names (basenames under shots/), sorted. */
  shots: string[];
  /** The last lines of run.log, path-redacted; null when the file is absent. */
  logTail: string[] | null;
  /** Total run.log lines after trailing-blank trim; null when the file is absent. */
  logLines: number | null;
  /** One line per top-level data.json field: name and an honest shape/value. */
  dataSummary: { key: string; value: string }[] | null;
};

function summariseValue(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "string") {
    return value.length > 160 ? `${value.slice(0, 160)}…` : value;
  }
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) return `${value.length} item${value.length === 1 ? "" : "s"}`;
  if (typeof value === "object") return `${Object.keys(value).length} fields`;
  return typeof value;
}

/** The log's content is evidence; the host directory layout is not. Redacts
 *  any occurrence of the absolute evidence path, and strips the run root two
 *  levels up so sibling paths (manifests/…) shed the build machine's scratch
 *  layout too. Applied to the tail AND to any served text file. */
export function redactEvidenceText(dir: string, text: string): string {
  return text
    .replaceAll(dir, basenameOf(dir))
    .replaceAll(resolve(dir, "..", "..") + sep, "");
}

/** Reads what actually survives in the evidence directory. Never invents a
 *  file: an unreadable or missing piece is null/absent in the listing. */
export async function inspectEvidenceDir(dir: string): Promise<EvidenceListing> {
  const gone: EvidenceListing = {
    present: false,
    shots: [],
    logTail: null,
    logLines: null,
    dataSummary: null,
  };
  let stat;
  try {
    stat = await fs.stat(dir);
  } catch {
    return gone;
  }
  if (!stat.isDirectory()) return gone;

  let shots: string[] = [];
  try {
    shots = (await fs.readdir(join(dir, "shots")))
      .filter((name) => SHOT_EXT.test(name))
      .sort();
  } catch {
    shots = [];
  }

  let logTail: string[] | null = null;
  let logLines: number | null = null;
  try {
    const text = await fs.readFile(join(dir, "run.log"), "utf8");
    const lines = redactEvidenceText(dir, text).split("\n");
    while (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
    logLines = lines.length;
    logTail = lines.slice(-LOG_TAIL_LINES);
  } catch {
    logTail = null;
    logLines = null;
  }

  let dataSummary: { key: string; value: string }[] | null = null;
  try {
    const parsed: unknown = JSON.parse(await fs.readFile(join(dir, "data.json"), "utf8"));
    if (parsed !== null && typeof parsed === "object" && !Array.isArray(parsed)) {
      dataSummary = Object.entries(parsed as Record<string, unknown>).map(
        ([key, value]) => ({ key, value: summariseValue(value) }),
      );
    }
  } catch {
    dataSummary = null;
  }

  return { present: true, shots, logTail, logLines, dataSummary };
}

/** Up to `limit` items evenly spaced across the list, first and last kept —
 *  the filmstrip sampler. Order is preserved; a short list returns whole. */
export function sampleEvenly<T>(items: readonly T[], limit: number): T[] {
  if (limit <= 0 || items.length === 0) return [];
  if (items.length <= limit) return [...items];
  if (limit === 1) return [items[0]];
  const out: T[] = [];
  for (let i = 0; i < limit; i++) {
    out.push(items[Math.round((i * (items.length - 1)) / (limit - 1))]);
  }
  return out;
}

function basenameOf(path: string): string {
  const seg = path.replace(/[/\\]+$/, "").split(/[/\\]/).pop();
  return seg || path;
}

const SAFE_SEGMENT = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

/** Maps a client-supplied relative path to an absolute file inside the
 *  evidence directory, or null for anything that steps outside it. */
export function safeEvidenceFile(dir: string, rel: string): string | null {
  const segments = rel.split("/");
  if (segments.length === 0 || segments.some((s) => !SAFE_SEGMENT.test(s))) {
    return null;
  }
  const abs = resolve(dir, ...segments);
  return abs.startsWith(resolve(dir) + sep) ? abs : null;
}

export const EVIDENCE_CONTENT_TYPES: Record<string, string> = {
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".png": "image/png",
  ".webp": "image/webp",
  ".gif": "image/gif",
  ".log": "text/plain; charset=utf-8",
  ".txt": "text/plain; charset=utf-8",
  ".json": "application/json; charset=utf-8",
};

/** Frames that actually survive in a report's evidence directory, sampled
 *  evenly and mapped to the evidence-file route — raw paths stay server-side.
 *  Shared by the game page and the runs console so their filmstrips agree. */
export const FILMSTRIP_FRAMES = 6;

export async function reportShotHrefs(report: {
  id: string;
  evidencePath: string | null;
}): Promise<string[]> {
  if (!report.evidencePath) return [];
  const listing = await inspectEvidenceDir(report.evidencePath);
  if (!listing.present || listing.shots.length === 0) return [];
  return sampleEvenly(listing.shots, FILMSTRIP_FRAMES).map(
    (name) =>
      `/foundry/console/evidence/${encodeURIComponent(report.id)}/file/shots/${encodeURIComponent(name)}`,
  );
}
