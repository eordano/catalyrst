import {
  FoundryStateError,
  assertRate,
  getPool,
  logAction,
  withTx,
} from "./db.server";
import {
  getGddDoc,
  getSupersededBy,
  parseHonesty,
  replaceGddSection,
  splitGddSections,
  upsertGddDoc,
  type ParsedGddDoc,
} from "./gdd.server";
import type { GddDoc } from "./types";

// Editing never rewrites history: a save mints version n+1 with `supersedes`
// pointing at the doc that was edited, so the original — imported or not —
// stays verbatim and the old page grows its "superseded by" crumb. That is
// what makes the open-to-any-visitor write posture safe here: nothing a
// session writes can destroy what another session (or an import) wrote.

const MAX_BODY_BYTES = 256 * 1024;

/** vN ids grow in place (`zoo-v1` → `zoo-v2`); an id that never carried a
 *  version suffix gains one. Exposed pure for tests. */
export function nextGddId(id: string, version: number): string {
  return `${id.replace(/-v\d+$/, "")}-v${version}`;
}

/** The successor row for an edited body: same identity fields, bumped
 *  version, session provenance, honesty recounted from the new text. The
 *  hypothesis log rides along — experiment files are shipped beside a doc,
 *  not stored in its body, so an edit cannot change them. */
export function editedGddDoc(old: GddDoc, bodyMd: string): ParsedGddDoc {
  const version = old.version + 1;
  return {
    id: nextGddId(old.id, version),
    title: old.title,
    kind: old.kind,
    sceneId: old.sceneId,
    version,
    supersedes: old.id,
    source: "session",
    sourceRef: `edited from ${old.id}`,
    bodyMd,
    honesty: parseHonesty(bodyMd),
    hypotheses: old.hypotheses,
    groundsCell: old.groundsCell,
    groundingRequestIds: old.groundingRequestIds,
    createdAt: new Date().toISOString(),
  };
}

export interface EditGddDocInput {
  docId: string;
  /** Whole-document edit; ignored when `section` is present. */
  bodyMd?: string;
  /** One-section edit, spliced into the current body byte-faithfully. */
  section?: { index: number; name?: string; contentMd: string };
  sid: string;
  ip?: string | null;
}

export interface EditedGddDoc {
  id: string;
  title: string;
  version: number;
}

export async function editGddDoc(input: EditGddDocInput): Promise<EditedGddDoc> {
  const { docId, section, sid, ip } = input;
  assertRate(sid, ip);

  const db = getPool();
  const old = await getGddDoc(db, docId);
  if (!old) throw new FoundryStateError("No such design document.");

  const successor = await getSupersededBy(db, docId);
  if (successor) {
    throw new FoundryStateError(
      `This document was already superseded by v${successor.version} — edit that one instead.`,
    );
  }

  let bodyMd: string;
  if (section) {
    const sections = splitGddSections(old.bodyMd);
    const current =
      Number.isInteger(section.index) && section.index >= 0
        ? sections[section.index]
        : undefined;
    if (!current) {
      throw new FoundryStateError("That section is not part of this document.");
    }
    if (section.name !== undefined && section.name !== current.name) {
      throw new FoundryStateError(
        "The document changed while you were editing — reload and try again.",
      );
    }
    bodyMd = replaceGddSection(old.bodyMd, section.index, section.contentMd);
  } else {
    bodyMd = (input.bodyMd ?? "").replace(/\r\n/g, "\n");
  }

  if (!bodyMd.trim()) {
    throw new FoundryStateError("An empty document cannot supersede this one.");
  }
  if (Buffer.byteLength(bodyMd, "utf8") > MAX_BODY_BYTES) {
    throw new FoundryStateError(
      "That edit is larger than a design document can be (256 KiB).",
    );
  }
  if (bodyMd === old.bodyMd) {
    throw new FoundryStateError(
      "Nothing changed — the edit matches the current version.",
    );
  }

  const doc = editedGddDoc(old, bodyMd);
  // upsertGddDoc overwrites on id conflict by design (imports refresh in
  // place); an edit must never inherit that, so the successor id has to be
  // unclaimed. Reachable only through id-suffix collisions the supersededBy
  // guard cannot see.
  if (await getGddDoc(db, doc.id)) {
    throw new FoundryStateError(
      `A document with id ${doc.id} already exists — this edit cannot supersede safely.`,
    );
  }

  await withTx(async (client) => {
    await upsertGddDoc(client, doc);
    await logAction(client, {
      sid,
      action: "edit_gdd_doc",
      subject: doc.id,
      detail: {
        from: old.id,
        section: section ? section.index : null,
        bytes: Buffer.byteLength(bodyMd, "utf8"),
      },
    });
  });
  return { id: doc.id, title: doc.title, version: doc.version };
}
