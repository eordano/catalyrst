import {
  FoundryStateError,
  assertRate,
  logAction,
  withTx,
} from "./db.server";
import {
  copilotSessionMessages,
  isCopilotSessionId,
} from "./copilot.server";
import { parseGddText, upsertGddDoc, type ParsedGddDoc } from "./gdd.server";

// Publishing a copilot draft without an operator: the server pulls the document
// straight out of the copilot session's own transcript — over the same socket
// and credential the status probe already uses — so the visitor supplies only
// the session id, never the content. What lands in foundry.gdd_doc is verbatim
// what the copilot said, provenance-stamped with the session it said it in.

const FENCE_RE = /```(?:markdown|md)?\r?\n([\s\S]*?)\r?\n```/g;

const NO_DOC =
  "No finished design document found in that session. Ask the copilot to emit " +
  "the document with the emit_document tool (or as one fenced block), then " +
  "publish again.";

type TranscriptPart = {
  type?: unknown;
  text?: unknown;
  tool?: unknown;
  state?: { status?: unknown; output?: unknown };
};
type TranscriptMessage = {
  info?: { role?: unknown; time?: { completed?: unknown } };
  parts?: TranscriptPart[];
};

/**
 * The newest fenced markdown block that looks like a design document —
 * frontmatter or headings, not a stray code sample — from the session's
 * COMPLETED assistant messages. Exposed pure for tests.
 */
export function draftFromTranscript(messages: unknown): string | null {
  if (!Array.isArray(messages)) return null;
  for (const m of [...(messages as TranscriptMessage[])].reverse()) {
    const info = m?.info;
    if (info?.role !== "assistant" || !info?.time?.completed) continue;

    // Preferred source: a completed emit_document MCP call — its output is
    // the schema-validated assembled markdown (the SAVED preamble stripped;
    // the document starts at the first frontmatter line).
    for (const p of [...(m.parts ?? [])].reverse()) {
      if (
        p?.type === "tool" &&
        typeof p.tool === "string" &&
        p.tool.endsWith("emit_document") &&
        p.state?.status === "completed" &&
        typeof p.state.output === "string"
      ) {
        const out = p.state.output;
        const start = out.indexOf("---\n");
        if (start === -1) continue;
        const doc = out.slice(start).trim();
        const headings = (doc.match(/^## /gm) ?? []).length;
        if (headings >= 5) return doc;
      }
    }

    const text = (m.parts ?? [])
      .filter((p) => p?.type === "text" && typeof p.text === "string")
      .map((p) => p.text as string)
      .join("\n");
    const blocks = [...text.matchAll(FENCE_RE)].map((b) => b[1].trim());
    for (const block of blocks.reverse()) {
      const headings = (block.match(/^## /gm) ?? []).length;
      if (headings >= 5) return block;
    }
  }
  return null;
}

export interface PublishedDraft {
  id: string;
  title: string;
  sections: number;
}

export interface PublishDraftInput {
  sessionId: string;
  sid: string;
  ip?: string | null;
}

export async function publishCopilotDraft(
  input: PublishDraftInput,
): Promise<PublishedDraft> {
  const { sessionId, sid, ip } = input;
  assertRate(sid, ip);

  const trimmed = sessionId.trim();
  if (!isCopilotSessionId(trimmed)) {
    throw new FoundryStateError(
      "That does not look like a copilot session id — it reads ses_ followed by letters and digits, from the copilot's own address bar.",
    );
  }

  let transcript: unknown;
  try {
    transcript = await copilotSessionMessages(trimmed);
  } catch {
    throw new FoundryStateError(
      "The copilot did not answer just now, so the session could not be read. Try again in a minute.",
    );
  }

  const raw = draftFromTranscript(transcript);
  if (!raw) throw new FoundryStateError(NO_DOC);

  // A document that skipped the frontmatter but opens with `# Name` still
  // names itself — that heading is the copilot's own words, not an invention.
  const h1 = /^# (.+)$/m.exec(raw)?.[1]?.trim();

  let doc: ParsedGddDoc;
  try {
    doc = parseGddText(raw, {
      source: "copilot",
      sourceRef: `copilot session ${trimmed}`,
      label: `session ${trimmed}`,
      ...(h1 ? { fallbackTitle: h1 } : {}),
    });
  } catch (e) {
    const why =
      e instanceof Error ? e.message : "The document in that session did not parse.";
    throw new FoundryStateError(
      `${why}. Ask the copilot to re-emit the document starting with a frontmatter block — ---, title: <name>, --- — then publish again.`,
    );
  }

  const sections = doc.honesty.sections.length;
  await withTx(async (client) => {
    await upsertGddDoc(client, doc);
    await logAction(client, {
      sid,
      action: "publish_gdd_draft",
      subject: doc.id,
      detail: { session: trimmed, title: doc.title, sections },
    });
  });
  return { id: doc.id, title: doc.title, sections };
}
