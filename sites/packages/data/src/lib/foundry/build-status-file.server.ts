import { promises as fs } from "node:fs";
import { join, resolve, sep } from "node:path";

import { copilotRoot } from "./copilot.server";

// While the Forge broker works a build it drops
// projects/<slug>/build/status.json into the copilot workspace read bind. This
// module reads that one file and nothing else -- no DB, no writes. The
// build_request row stays the state machine's source of truth; a stored file
// is display detail beside it, and an absent or malformed one is null, never
// an invented step.

export type BuildStatusFile = {
  step: string;
  note: string;
  updated: string | null;
};

const SLUG_RE = /^[a-z0-9-]{1,48}$/;

export async function readBuildStatusFile(
  slug: string,
  root?: string,
): Promise<BuildStatusFile | null> {
  if (!SLUG_RE.test(slug)) return null;
  const base = root ?? copilotRoot();
  const rootAbs = resolve(base);
  const path = resolve(base, join("projects", slug, "build", "status.json"));
  if (!path.startsWith(rootAbs + sep)) return null;
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
  if (typeof p.step !== "string" || p.step.trim() === "") return null;
  return {
    step: p.step.trim(),
    note: typeof p.note === "string" ? p.note : "",
    updated: typeof p.updated === "string" ? p.updated : null,
  };
}
