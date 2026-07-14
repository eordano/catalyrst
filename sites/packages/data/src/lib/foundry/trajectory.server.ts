import { randomUUID } from "node:crypto";

import type { PoolClient, QueryResult, QueryResultRow } from "pg";

import type {
  Trajectory,
  TrajectoryEvent,
  TrajectoryEventType,
  TrajectoryProvenance,
  TurnEndReason,
} from "./types";

// Event vocabulary, contiguity, fork-at-seq and the refuse-unknown rule are
// adapted from deepseek-ai/deepseek-harness (MIT) session persistence:
// docs/subsystems/session.md, docs/subsystems/persistence.md.

export interface FoundryDb {
  query<R extends QueryResultRow = QueryResultRow>(
    text: string,
    values?: unknown[],
  ): Promise<QueryResult<R>>;
}

type PoolLike = FoundryDb & { connect(): Promise<PoolClient> };

function isPool(db: FoundryDb): db is PoolLike {
  const candidate = db as Partial<PoolLike> & { release?: unknown };
  return (
    typeof candidate.connect === "function" &&
    typeof candidate.release !== "function"
  );
}

// A caller that hands us a client owns the transaction; a pool means we open one.
async function inTx<T>(
  db: FoundryDb,
  fn: (client: FoundryDb) => Promise<T>,
): Promise<T> {
  if (!isPool(db)) return fn(db);
  const client = await db.connect();
  try {
    await client.query("BEGIN");
    const out = await fn(client);
    await client.query("COMMIT");
    return out;
  } catch (err) {
    try {
      await client.query("ROLLBACK");
    } catch {
      // the connection is already gone; the original error is the useful one
    }
    throw err;
  } finally {
    client.release();
  }
}

export class TrajectorySeqError extends Error {
  readonly status = 409;
  constructor(message: string) {
    super(message);
    this.name = "TrajectorySeqError";
  }
}

export class TrajectoryForkError extends Error {
  readonly status = 409;
  constructor(message: string) {
    super(message);
    this.name = "TrajectoryForkError";
  }
}

export class TrajectoryLogError extends Error {
  readonly status = 422;
  constructor(message: string) {
    super(message);
    this.name = "TrajectoryLogError";
  }
}

export const REPLAY_EVENT_LIMIT = 20_000;

export const TRAJECTORY_EVENT_TYPES: readonly TrajectoryEventType[] = [
  "turn/start",
  "turn/end",
  "step/start",
  "step/end",
  "tool/call",
  "tool/result",
  "obs/snapshot",
  "check/verdict",
  "run/end-seed",
];

const KNOWN_TYPES = new Set<string>(TRAJECTORY_EVENT_TYPES);

export type TrajectoryListRow = Trajectory & {
  events: number;
  sceneTitle: string | null;
};

function assertLossless(value: unknown, path: string, seen: Set<object>): void {
  if (value === null) return;
  switch (typeof value) {
    case "string":
    case "boolean":
      return;
    case "number":
      if (!Number.isFinite(value)) {
        throw new TrajectoryLogError(`${path} is ${String(value)}; event data must be lossless JSON`);
      }
      return;
    case "bigint":
    case "function":
    case "symbol":
    case "undefined":
      throw new TrajectoryLogError(
        `${path} is a ${typeof value}; event data must be lossless JSON`,
      );
  }
  const obj = value as object;
  if (seen.has(obj)) {
    throw new TrajectoryLogError(`${path} is circular; event data must be lossless JSON`);
  }
  seen.add(obj);
  if (Array.isArray(obj)) {
    obj.forEach((item, i) => assertLossless(item, `${path}[${i}]`, seen));
    seen.delete(obj);
    return;
  }
  const proto = Object.getPrototypeOf(obj);
  if (proto !== Object.prototype && proto !== null) {
    throw new TrajectoryLogError(
      `${path} is a ${obj.constructor?.name ?? "non-plain object"}; event data must be lossless JSON`,
    );
  }
  for (const [key, item] of Object.entries(obj)) {
    assertLossless(item, `${path}.${key}`, seen);
  }
  seen.delete(obj);
}

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : new Date(value).toISOString();
}

type TrajectoryDbRow = {
  id: string;
  scene_id: string | null;
  provenance: string;
  runner: string | null;
  finish_reason: TurnEndReason | null;
  parent_trajectory_id: string | null;
  seed_length: number | null;
  evidence_path: string | null;
  created_at: Date | string;
};

const HEADER_COLUMNS = `t.id, t.scene_id, t.provenance, t.runner, t.finish_reason,
       t.parent_trajectory_id, t.seed_length, t.evidence_path, t.created_at`;

function toTrajectory(r: TrajectoryDbRow): Trajectory {
  return {
    id: r.id,
    sceneId: r.scene_id,
    provenance: r.provenance as TrajectoryProvenance,
    runner: r.runner as Trajectory["runner"],
    finishReason: r.finish_reason,
    parentTrajectoryId: r.parent_trajectory_id,
    seedLength: r.seed_length === null ? null : Number(r.seed_length),
    evidencePath: r.evidence_path,
    createdAt: iso(r.created_at),
  };
}

type EventDbRow = {
  seq: number;
  type: string;
  time: Date | string;
  data: unknown;
  ignorable: boolean | null;
};

function toEvent(trajectoryId: string, r: EventDbRow): TrajectoryEvent {
  if (!KNOWN_TYPES.has(r.type) && r.ignorable !== true) {
    throw new TrajectoryLogError(
      `trajectory ${trajectoryId} seq ${r.seq} has unknown event type "${r.type}" and is not marked ignorable — refusing to reconstruct the log`,
    );
  }
  const event: TrajectoryEvent = {
    trajectoryId,
    seq: Number(r.seq),
    type: r.type as TrajectoryEventType,
    time: iso(r.time),
    data: r.data,
  };
  return r.ignorable === true ? { ...event, ignorable: true } : event;
}

export async function createTrajectory(
  db: FoundryDb,
  t: Omit<Trajectory, "createdAt">,
  opts: { createdAt?: string } = {},
): Promise<void> {
  await db.query(
    `INSERT INTO foundry.trajectory
       (id, scene_id, provenance, runner, finish_reason,
        parent_trajectory_id, seed_length, evidence_path, created_at)
     VALUES ($1, $2, $3, $4, $5::jsonb, $6, $7, $8,
             coalesce($9::timestamptz, now()))
     ON CONFLICT (id) DO NOTHING`,
    [
      t.id,
      t.sceneId,
      t.provenance,
      t.runner,
      t.finishReason === null ? null : JSON.stringify(t.finishReason),
      t.parentTrajectoryId,
      t.seedLength,
      t.evidencePath,
      opts.createdAt ?? null,
    ],
  );
}

export async function appendEvents(
  db: FoundryDb,
  trajectoryId: string,
  events: readonly Omit<TrajectoryEvent, "trajectoryId">[],
): Promise<void> {
  if (events.length === 0) return;
  for (const e of events) {
    if (!Number.isInteger(e.seq) || e.seq < 0) {
      throw new TrajectorySeqError(`seq must be a non-negative integer, got ${String(e.seq)}`);
    }
    if (!KNOWN_TYPES.has(e.type) && e.ignorable !== true) {
      throw new TrajectoryLogError(
        `event type "${e.type}" is unknown and not marked ignorable — a later reader would have to refuse the whole log`,
      );
    }
    assertLossless(e.data, `seq ${e.seq} data`, new Set());
  }

  await inTx(db, async (client) => {
    for (const e of events) {
      const res = await client.query(
        `INSERT INTO foundry.trajectory_event
           (trajectory_id, seq, type, time, data, ignorable)
         SELECT $1, $2::int, $3, $4::timestamptz, $5::jsonb, $6::boolean
          WHERE $2::int = (SELECT coalesce(max(seq) + 1, 0)
                             FROM foundry.trajectory_event
                            WHERE trajectory_id = $1)`,
        [
          trajectoryId,
          e.seq,
          e.type,
          e.time,
          JSON.stringify(e.data ?? null),
          e.ignorable === true ? true : null,
        ],
      );
      if (res.rowCount !== 1) {
        throw new TrajectorySeqError(
          `trajectory ${trajectoryId}: seq ${e.seq} is not the next contiguous seq`,
        );
      }
    }
  });
}

export async function listTrajectories(
  db: FoundryDb,
  opts: {
    provenance?: TrajectoryProvenance;
    sceneId?: string;
    limit?: number;
  } = {},
): Promise<TrajectoryListRow[]> {
  const res = await db.query<TrajectoryDbRow & { scene_title: string | null; events: number }>(
    `SELECT ${HEADER_COLUMNS},
            s.title AS scene_title,
            (SELECT count(*) FROM foundry.trajectory_event e
              WHERE e.trajectory_id = t.id)::int AS events
       FROM foundry.trajectory t
       LEFT JOIN foundry.scene s ON s.id = t.scene_id
      WHERE ($1::text IS NULL OR t.provenance = $1)
        AND ($2::text IS NULL OR t.scene_id = $2)
      ORDER BY t.created_at DESC, t.id DESC
      LIMIT $3`,
    [opts.provenance ?? null, opts.sceneId ?? null, opts.limit ?? 100],
  );
  return res.rows.map((r) => ({
    ...toTrajectory(r),
    sceneTitle: r.scene_title,
    events: Number(r.events),
  }));
}

/** Total episodes on record, so a capped list can say "showing N of M". */
export async function countTrajectories(
  db: FoundryDb,
  opts: { provenance?: TrajectoryProvenance; sceneId?: string } = {},
): Promise<number> {
  const res = await db.query<{ total: number }>(
    `SELECT count(*)::int AS total
       FROM foundry.trajectory t
      WHERE ($1::text IS NULL OR t.provenance = $1)
        AND ($2::text IS NULL OR t.scene_id = $2)`,
    [opts.provenance ?? null, opts.sceneId ?? null],
  );
  return Number(res.rows[0]?.total ?? 0);
}

export async function getTrajectoryHeader(
  db: FoundryDb,
  id: string,
): Promise<(Trajectory & { sceneTitle: string | null }) | null> {
  const res = await db.query<TrajectoryDbRow & { scene_title: string | null }>(
    `SELECT ${HEADER_COLUMNS}, s.title AS scene_title
       FROM foundry.trajectory t
       LEFT JOIN foundry.scene s ON s.id = t.scene_id
      WHERE t.id = $1`,
    [id],
  );
  const row = res.rows[0];
  return row ? { ...toTrajectory(row), sceneTitle: row.scene_title } : null;
}

export async function countTrajectoryEvents(
  db: FoundryDb,
  id: string,
): Promise<number> {
  const res = await db.query<{ events: number }>(
    `SELECT count(*)::int AS events
       FROM foundry.trajectory_event WHERE trajectory_id = $1`,
    [id],
  );
  return Number(res.rows[0]?.events ?? 0);
}

async function readEvents(
  db: FoundryDb,
  trajectoryId: string,
  throughSeq?: number,
): Promise<TrajectoryEvent[]> {
  const res = await db.query<EventDbRow>(
    `SELECT seq, type, time, data, ignorable
       FROM foundry.trajectory_event
      WHERE trajectory_id = $1
        AND ($2::int IS NULL OR seq <= $2::int)
      ORDER BY seq`,
    [trajectoryId, throughSeq ?? null],
  );
  return res.rows.map((r) => toEvent(trajectoryId, r));
}

export async function listTrajectoryEvents(
  db: FoundryDb,
  trajectoryId: string,
): Promise<TrajectoryEvent[]> {
  return readEvents(db, trajectoryId);
}

export async function getTrajectory(
  db: FoundryDb,
  id: string,
): Promise<{ header: Trajectory; events: TrajectoryEvent[] } | null> {
  const header = await getTrajectoryHeader(db, id);
  if (!header) return null;
  return { header, events: await readEvents(db, id) };
}

/** dsh fork rule: the copied prefix must end outside an open turn or step. */
function assertClosedBrackets(events: readonly TrajectoryEvent[]): void {
  let turns = 0;
  let steps = 0;
  for (const e of events) {
    if (e.type === "turn/start") turns += 1;
    if (e.type === "turn/end") turns -= 1;
    if (e.type === "step/start") steps += 1;
    if (e.type === "step/end") steps -= 1;
  }
  if (turns !== 0 || steps !== 0) {
    throw new TrajectoryForkError(
      `fork boundary falls inside an open ${turns !== 0 ? "turn" : "step"} — pick a seq at a closed bracket`,
    );
  }
}

export async function forkTrajectory(
  db: FoundryDb,
  sourceId: string,
  atSeq: number,
): Promise<string> {
  if (!Number.isInteger(atSeq) || atSeq < 0) {
    throw new TrajectoryForkError(`fork boundary must be a non-negative seq, got ${String(atSeq)}`);
  }
  return inTx(db, async (client) => {
    const header = await getTrajectoryHeader(client, sourceId);
    if (!header) throw new TrajectoryForkError(`no trajectory ${sourceId}`);

    const prefix = await readEvents(client, sourceId, atSeq);
    if (prefix.length !== atSeq + 1) {
      throw new TrajectoryForkError(
        `trajectory ${sourceId} has ${prefix.length} events through seq ${atSeq}; expected ${atSeq + 1}`,
      );
    }
    assertClosedBrackets(prefix);

    const childId = randomUUID();
    await createTrajectory(client, {
      id: childId,
      sceneId: header.sceneId,
      provenance: header.provenance,
      runner: header.runner,
      finishReason: null,
      parentTrajectoryId: sourceId,
      seedLength: atSeq + 1,
      evidencePath: header.evidencePath,
    });
    await appendEvents(
      client,
      childId,
      prefix.map((e) => ({
        seq: e.seq,
        type: e.type,
        time: e.time,
        data: e.data,
        ...(e.ignorable === true ? { ignorable: true as const } : {}),
      })),
    );
    await appendEvents(client, childId, [
      {
        seq: atSeq + 1,
        type: "run/end-seed",
        time: new Date().toISOString(),
        data: {},
      },
    ]);
    return childId;
  });
}

export function deriveFinishReason(
  events: readonly TrajectoryEvent[],
): TurnEndReason | null {
  for (let i = events.length - 1; i >= 0; i -= 1) {
    const e = events[i];
    if (e.type !== "turn/end") continue;
    const reason = (e.data as { reason?: TurnEndReason } | null)?.reason;
    return reason ?? null;
  }
  return null;
}

export interface TrajectorySample {
  generatedFrom: {
    command: string;
    cwd: string;
    repoCommit: string;
    runtime: string;
    startedAt: string;
    finishedAt: string;
    exitCode: number;
    mapping: string;
  };
  trajectory: Trajectory;
  events: Omit<TrajectoryEvent, "trajectoryId">[];
}

export async function importTrajectorySample(
  db: FoundryDb,
  sample: TrajectorySample,
): Promise<string> {
  const { trajectory, events } = sample;
  await inTx(db, async (client) => {
    await createTrajectory(client, trajectory, { createdAt: trajectory.createdAt });
    const already = await countTrajectoryEvents(client, trajectory.id);
    if (already === 0) await appendEvents(client, trajectory.id, events);
  });
  return trajectory.id;
}
