import type { Pool } from "pg";

import { listBenchReports } from "./bench.server";
import { SCENE_ACTIONS, listStewards, listTransfers } from "./continuity.server";
import { sidBadge } from "./db.server";
import { getGddDoc } from "./gdd.server";
import { getScene } from "./scenes.server";
import { listTrajectories, listTrajectoryEvents } from "./trajectory.server";
import type {
  ChangelogRow,
  ContinuitySummary,
  GddDoc,
  Origin,
  ProjectBundle,
} from "./types";

// Assembles a faithful, downloadable record of one scene. It is faithful in both
// directions: it hides nothing that was recorded (arena bench runs are INCLUDED,
// unlike the demand counters), and it invents nothing (empty sections are empty
// arrays, never filler). Two things are deliberately withheld — the scene bytes
// (this is a record, not a redeploy) and any secret: a transfer's token_hash is
// never read here, and every sid is reduced to its badge so the bundle carries
// no raw session identity. The route edge additionally labels filesystem paths.

const EVENT_CAP = 2000;

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : String(value);
}

function originOf(value: string): Origin {
  if (value === "visitor") return "visitor";
  if (value === "recorded") return "recorded";
  return "import";
}

export async function buildProjectBundle(
  db: Pool,
  sceneId: string,
): Promise<ProjectBundle> {
  const scene = await getScene(db, sceneId);

  const changelogRes = await db.query<{
    at: Date | string;
    note: string;
    source_note: string;
    origin: string;
  }>(
    `SELECT at, note, source_note, origin
       FROM foundry.scene_changelog
      WHERE scene_id = $1
      ORDER BY at DESC`,
    [sceneId],
  );
  const changelog: ChangelogRow[] = changelogRes.rows.map((r) => ({
    at: iso(r.at),
    note: r.note,
    sourceNote: r.source_note,
    origin: originOf(r.origin),
  }));

  const docIds = await db.query<{ id: string }>(
    `SELECT id FROM foundry.gdd_doc WHERE scene_id = $1 ORDER BY created_at, version`,
    [sceneId],
  );
  const docs: GddDoc[] = [];
  for (const { id } of docIds.rows) {
    const doc = await getGddDoc(db, id);
    if (doc) docs.push(doc);
  }

  // Every bench report on the scene, arena included — the demand counters filter
  // arena, but a record that dropped rows would not be a record.
  const reports = await listBenchReports(db, sceneId, 10_000);

  const headers = await listTrajectories(db, { sceneId, limit: 10_000 });
  const trajectories: ProjectBundle["trajectories"] = [];
  for (const header of headers) {
    const events = await listTrajectoryEvents(db, header.id);
    const exported = events.slice(0, EVENT_CAP).map((e) => ({
      seq: e.seq,
      type: e.type,
      time: e.time,
      data: e.data,
      ...(e.ignorable === true ? { ignorable: true as const } : {}),
    }));
    trajectories.push({
      trajectory: {
        id: header.id,
        sceneId: header.sceneId,
        provenance: header.provenance,
        runner: header.runner,
        finishReason: header.finishReason,
        parentTrajectoryId: header.parentTrajectoryId,
        seedLength: header.seedLength,
        evidencePath: header.evidencePath,
        createdAt: header.createdAt,
      },
      events: exported,
      eventsStored: events.length,
      eventsExported: exported.length,
      truncated: events.length > exported.length,
    });
  }

  const actionsRes = await db.query<{
    at: Date | string;
    sid: string;
    action: string;
    detail: Record<string, unknown>;
  }>(
    `SELECT at, sid, action, detail
       FROM foundry.action_log
      WHERE subject = $1 AND action = ANY($2::text[])
      ORDER BY at DESC`,
    [sceneId, [...SCENE_ACTIONS]],
  );
  const actions = actionsRes.rows.map((r) => ({
    at: iso(r.at),
    actor: sidBadge(r.sid),
    action: r.action,
    detail: r.detail ?? {},
  }));

  const stewardsList = await listStewards(sceneId);
  const stewards = [...stewardsList.active, ...stewardsList.past];
  const transfers = await listTransfers(sceneId);

  const migRes = await db.query<{ name: string }>(
    `SELECT name FROM foundry.foundry_migration ORDER BY name`,
  );

  return {
    provenance: {
      scene: sceneId,
      generatedAt: new Date().toISOString(),
      note: "Assembly time of this bundle on the server clock — not a deployment fact. No scene bytes and no secrets (transfer codes, session ids) are included; every actor is a claimed persona name or a session badge.",
    },
    scene,
    changelog,
    docs,
    reports,
    trajectories,
    actions,
    stewards,
    transfers,
    migrations: migRes.rows.map((r) => r.name),
  };
}

const SUMMARY_SQL = `
  SELECT (SELECT count(*)::int FROM foundry.scene_changelog WHERE scene_id = $1) AS changelog,
         (SELECT count(*)::int FROM foundry.bot_report
           WHERE scene_id = $1 AND runner IS DISTINCT FROM 'arena') AS reports,
         (SELECT count(*)::int FROM foundry.bot_report
           WHERE scene_id = $1) AS reports_all,
         (SELECT count(*)::int FROM foundry.trajectory WHERE scene_id = $1) AS episodes,
         (SELECT count(*)::int FROM foundry.gdd_doc WHERE scene_id = $1) AS docs,
         (SELECT count(*)::int FROM foundry.scene_steward
           WHERE scene_id = $1 AND released_at IS NULL) AS stewards`;

/** The counts line under a scene. Demand-bearing (reports) excludes arena;
 *  reportsAll is the bundle's own count, sandbox sims included; every field is a
 *  measured count that prints '0' when empty. */
export async function continuitySummary(
  db: Pool,
  sceneId: string,
): Promise<ContinuitySummary> {
  const res = await db.query<{
    changelog: number;
    reports: number;
    reports_all: number;
    episodes: number;
    docs: number;
    stewards: number;
  }>(SUMMARY_SQL, [sceneId]);
  const r = res.rows[0];
  return {
    changelog: Number(r?.changelog ?? 0),
    reports: Number(r?.reports ?? 0),
    reportsAll: Number(r?.reports_all ?? 0),
    episodes: Number(r?.episodes ?? 0),
    docs: Number(r?.docs ?? 0),
    stewards: Number(r?.stewards ?? 0),
  };
}
