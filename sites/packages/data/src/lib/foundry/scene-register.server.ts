import {
  FoundryStateError,
  assertRate,
  logAction,
  withTx,
} from "./db.server";
import { requireHost } from "./roles.server";

// The web writer for the registry, beside foundry:import-real. A registered
// game is a 'repo' row — the shape the checked-in template scene already has.
// Every mirror-read column (world_name, entity_id, deployed_at, size_bytes,
// parcels, description, thumbnail_url) stays NULL: those are deployment facts
// only the worlds import may fill (schema.sql's rule), and a hand-typed value
// there would be the fiction v3 exists to remove. What a host CAN honestly
// record is the row's identity and provenance — id, title, repo path, doc
// link, source note — and that is exactly this form.

export const SCENE_REGISTER_LIMITS = {
  id: 40,
  title: 80,
  repoPath: 200,
  gddDocId: 120,
  sourceNote: 280,
} as const;

const SLUG_RE = /^[a-z0-9][a-z0-9-]*$/;

function validate(input: {
  id: string;
  title: string;
  repoPath: string;
  gddDocId: string;
  sourceNote: string;
}): string | null {
  const id = input.id.trim();
  const title = input.title.trim();
  const note = input.sourceNote.trim();
  if (id.length === 0) return "Give the game an id — it becomes /foundry/play/<id>.";
  if (id.length > SCENE_REGISTER_LIMITS.id)
    return `Ids are ${SCENE_REGISTER_LIMITS.id} characters or fewer.`;
  if (!SLUG_RE.test(id))
    return "Ids are lowercase letters, digits and dashes, starting with a letter or digit.";
  // /foundry/play/register is this form's own address, so a game under that id
  // would have no reachable page.
  if (id === "register") return 'The id "register" is taken by this form — pick another.';
  if (title.length === 0) return "Give the game a title.";
  if (title.length > SCENE_REGISTER_LIMITS.title)
    return `Titles are ${SCENE_REGISTER_LIMITS.title} characters or fewer.`;
  // The shelf shows every card's provenance — a row registered without one
  // would be the first card that cannot say where it came from.
  if (note.length === 0)
    return "Say where this game comes from — the shelf shows it on the card.";
  if (note.length > SCENE_REGISTER_LIMITS.sourceNote)
    return `Source notes are ${SCENE_REGISTER_LIMITS.sourceNote} characters or fewer.`;
  if (input.repoPath.trim().length > SCENE_REGISTER_LIMITS.repoPath)
    return `Repo paths are ${SCENE_REGISTER_LIMITS.repoPath} characters or fewer.`;
  if (input.gddDocId.trim().length > SCENE_REGISTER_LIMITS.gddDocId)
    return `Doc ids are ${SCENE_REGISTER_LIMITS.gddDocId} characters or fewer.`;
  return null;
}

/**
 * Registers a game on the shelf. Host-gated in-tx; refuses a taken id and a
 * doc id that resolves to no document, out loud. Writes the same shape
 * importReal writes — a scene row plus its changelog row — and records the act
 * in action_log; the two writers coexist because neither touches the other's
 * rows (the import upserts only its own fixture ids).
 */
export async function registerScene({
  sid,
  id,
  title,
  repoPath,
  gddDocId,
  sourceNote,
  ip,
}: {
  sid: string;
  id: string;
  title: string;
  repoPath: string;
  gddDocId: string;
  sourceNote: string;
  ip?: string | null;
}): Promise<{ id: string }> {
  const error = validate({ id, title, repoPath, gddDocId, sourceNote });
  if (error) throw new FoundryStateError(error);
  const slug = id.trim();
  const name = title.trim();
  const repo = repoPath.trim();
  const doc = gddDocId.trim();
  const note = sourceNote.trim();
  assertRate(sid, ip);
  return withTx(async (client) => {
    await requireHost(client, sid);
    if (doc !== "") {
      const found = await client.query(
        `SELECT 1 FROM foundry.gdd_doc WHERE id = $1`,
        [doc],
      );
      if (found.rowCount === 0) {
        throw new FoundryStateError(
          `No design doc has the id "${doc}" — use an id from /foundry/gdd, or leave the field empty.`,
        );
      }
    }
    // ON CONFLICT DO NOTHING + RETURNING: a taken id yields zero rows, so the
    // duplicate refusal is race-safe rather than a pre-check that can be lost.
    const inserted = await client.query(
      `INSERT INTO foundry.scene (id, title, repo_path, gdd_doc_id, source, source_note)
       VALUES ($1, $2, $3, $4, 'repo', $5)
       ON CONFLICT (id) DO NOTHING
       RETURNING id`,
      [slug, name, repo === "" ? null : repo, doc === "" ? null : doc, note],
    );
    if (inserted.rowCount === 0) {
      throw new FoundryStateError(
        `"${slug}" is already on the shelf — pick another id, or open /foundry/play/${slug}.`,
      );
    }
    await client.query(
      `INSERT INTO foundry.scene_changelog (scene_id, at, note, source_note, origin, sid)
       VALUES ($1, now(), $2, $3, 'visitor', $4)`,
      [slug, "Registered on the shelf", note, sid],
    );
    await logAction(client, {
      sid,
      action: "register_scene",
      subject: slug,
      detail: { title: name },
    });
    return { id: slug };
  });
}
