import { promises as fs } from "node:fs";
import { join, resolve, sep } from "node:path";

import { copilotRoot } from "./copilot.server";

// The copilot's staged design pipeline leaves its state on disk:
// projects/<slug>/pipeline/pipeline.json plus the step artifacts it names.
// This module reads that tree and nothing else — no DB, no writes. Artifact
// paths stay workspace-relative; the absolute workspace root never leaves the
// server.

export type PipelineStepId = "intake" | "interview" | "draft" | "evidence";

export type PipelineStepRecord = {
  id: PipelineStepId;
  status: "pending" | "passed" | "failed";
  artifact: string | null;
  problems: string[];
  updated: string | null;
};

export type PipelineSummary = {
  slug: string;
  title: string;
  kind: string;
  created: string;
  passed: number;
  total: number;
  /** First non-passed step; null when every step passed. */
  next: PipelineStepId | null;
};

export type PipelineStepView = PipelineStepRecord & {
  /** Artifact file text; null when artifact is null or the file is unreadable. */
  content: string | null;
};

export type PipelineDetail = {
  slug: string;
  title: string;
  kind: string;
  created: string;
  steps: PipelineStepView[];
};

const SLUG_RE = /^[a-z0-9-]{1,48}$/;

type StoredPipeline = {
  slug: string;
  title: string;
  kind: string;
  created: string;
  steps: PipelineStepRecord[];
};

function toStepRecord(raw: unknown): PipelineStepRecord {
  const s = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  return {
    id: String(s.id ?? "") as PipelineStepId,
    status: (s.status === "passed" || s.status === "failed" ? s.status : "pending"),
    artifact: typeof s.artifact === "string" ? s.artifact : null,
    problems: Array.isArray(s.problems)
      ? s.problems.filter((p): p is string => typeof p === "string")
      : [],
    updated: typeof s.updated === "string" ? s.updated : null,
  };
}

async function readPipelineJson(path: string): Promise<StoredPipeline | null> {
  let text: string;
  try {
    text = await fs.readFile(path, "utf8");
  } catch {
    return null;
  }
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return null;
  }
  if (!raw || typeof raw !== "object") return null;
  const p = raw as Record<string, unknown>;
  if (typeof p.slug !== "string" || typeof p.title !== "string" || !Array.isArray(p.steps)) {
    return null;
  }
  return {
    slug: p.slug,
    title: p.title,
    kind: typeof p.kind === "string" ? p.kind : "",
    created: typeof p.created === "string" ? p.created : "",
    steps: p.steps.map(toStepRecord),
  };
}

function summarize(stored: StoredPipeline): PipelineSummary {
  const next = stored.steps.find((s) => s.status !== "passed");
  return {
    slug: stored.slug,
    title: stored.title,
    kind: stored.kind,
    created: stored.created,
    passed: stored.steps.filter((s) => s.status === "passed").length,
    total: stored.steps.length,
    next: next ? next.id : null,
  };
}

export async function listPipelines(root?: string): Promise<PipelineSummary[]> {
  const base = root ?? copilotRoot();
  let entries: string[];
  try {
    entries = await fs.readdir(join(base, "projects"));
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === "ENOENT") return [];
    throw err;
  }
  const summaries: PipelineSummary[] = [];
  for (const entry of entries) {
    const stored = await readPipelineJson(
      join(base, "projects", entry, "pipeline", "pipeline.json"),
    );
    if (stored) summaries.push(summarize(stored));
  }
  summaries.sort((a, b) => b.created.localeCompare(a.created));
  return summaries;
}

export async function readPipeline(
  slug: string,
  root?: string,
): Promise<PipelineDetail | null> {
  if (!SLUG_RE.test(slug)) return null;
  const base = root ?? copilotRoot();
  const stored = await readPipelineJson(
    join(base, "projects", slug, "pipeline", "pipeline.json"),
  );
  if (!stored) return null;

  const rootAbs = resolve(base);
  const steps: PipelineStepView[] = [];
  for (const step of stored.steps) {
    let content: string | null = null;
    if (step.artifact) {
      const artifactAbs = resolve(base, step.artifact);
      if (artifactAbs.startsWith(rootAbs + sep)) {
        content = await fs.readFile(artifactAbs, "utf8").catch(() => null);
      }
    }
    steps.push({ ...step, content });
  }

  return {
    slug: stored.slug,
    title: stored.title,
    kind: stored.kind,
    created: stored.created,
    steps,
  };
}
