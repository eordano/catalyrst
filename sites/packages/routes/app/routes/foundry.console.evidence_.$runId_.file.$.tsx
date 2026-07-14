import { promises as fs } from "node:fs";
import { extname } from "node:path";

import { FoundryUnavailableError, getPool } from "@data/lib/foundry/db.server";
import {
  EVIDENCE_CONTENT_TYPES,
  redactEvidenceText,
  resolveEvidenceRun,
  safeEvidenceFile,
} from "@data/lib/foundry/evidence.server";

import type { Route } from "./+types/foundry.console.evidence_.$runId_.file.$";

// Resource route: serves one surviving file out of a run's evidence directory,
// read-only. The client only ever names a relative path; anything that is not a
// plain path inside the recorded directory — or whose type is not on the small
// allowlist — is a 404, never a directory listing and never a traversal.

const MAX_BYTES = 20 * 1024 * 1024;

export async function loader({ params }: Route.LoaderArgs) {
  const notFound = () => new Response("Not found", { status: 404 });

  let run;
  try {
    run = await resolveEvidenceRun(getPool(), params.runId);
  } catch (err) {
    if (err instanceof FoundryUnavailableError) return notFound();
    throw err;
  }
  if (!run?.evidencePath) return notFound();

  const rel = params["*"] ?? "";
  const abs = safeEvidenceFile(run.evidencePath, rel);
  if (!abs) return notFound();

  const type = EVIDENCE_CONTENT_TYPES[extname(abs).toLowerCase()];
  if (!type) return notFound();

  let stat;
  try {
    stat = await fs.stat(abs);
  } catch {
    return notFound();
  }
  if (!stat.isFile() || stat.size > MAX_BYTES) return notFound();

  // A text file can quote the host's directory layout the same way the log
  // tail does; serve it through the same redaction so the absolute host path
  // never leaves the server on any route. Binary types ship byte-for-byte.
  if (type.startsWith("text/plain") || type.startsWith("application/json")) {
    const text = await fs.readFile(abs, "utf8");
    const body = redactEvidenceText(run.evidencePath, text);
    return new Response(body, {
      status: 200,
      headers: {
        "content-type": type,
        "content-length": String(Buffer.byteLength(body)),
        "cache-control": "private, max-age=60",
      },
    });
  }

  const body = await fs.readFile(abs);
  return new Response(new Uint8Array(body), {
    status: 200,
    headers: {
      "content-type": type,
      "content-length": String(stat.size),
      "cache-control": "private, max-age=60",
    },
  });
}
