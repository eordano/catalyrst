import { describe, expect, it } from "vitest";

import {
  appendEvents,
  createTrajectory,
  deriveFinishReason,
  forkTrajectory,
  getTrajectory,
  TrajectoryForkError,
  TrajectoryLogError,
  TrajectorySeqError,
  type FoundryDb,
} from "./trajectory.server";
import type { Trajectory, TrajectoryEvent } from "./types";

type HeaderRow = {
  id: string;
  scene_id: string | null;
  provenance: string;
  runner: string | null;
  finish_reason: unknown;
  parent_trajectory_id: string | null;
  seed_length: number | null;
  evidence_path: string | null;
  created_at: string;
  scene_title: string | null;
};

type EventRow = {
  trajectory_id: string;
  seq: number;
  type: string;
  time: string;
  data: unknown;
  ignorable: boolean | null;
};

/** Enough of Postgres to exercise the contiguity and fork rules in memory. */
function fakeDb() {
  const headers: HeaderRow[] = [];
  const events: EventRow[] = [];

  async function query(
    text: string,
    values: unknown[] = [],
  ): Promise<{ rows: unknown[]; rowCount: number }> {
    {
      if (text.includes("INSERT INTO foundry.trajectory\n")) {
        const [
          id,
          sceneId,
          provenance,
          runner,
          finishReason,
          parent,
          seedLength,
          evidencePath,
          createdAt,
        ] = values as (string | null)[];
        if (!headers.some((h) => h.id === id)) {
          headers.push({
            id: id as string,
            scene_id: sceneId,
            provenance: provenance as string,
            runner,
            finish_reason: finishReason === null ? null : JSON.parse(finishReason),
            parent_trajectory_id: parent,
            seed_length: seedLength === null ? null : Number(seedLength),
            evidence_path: evidencePath,
            created_at: createdAt ?? "2026-08-14T23:29:52.267Z",
            scene_title: "Flag Tag",
          });
        }
        return { rows: [], rowCount: 1 };
      }

      if (text.includes("INSERT INTO foundry.trajectory_event")) {
        const [trajectoryId, seq, type, time, data, ignorable] = values as [
          string,
          number,
          string,
          string,
          string,
          boolean | null,
        ];
        const next = events.filter((e) => e.trajectory_id === trajectoryId).length;
        if (Number(seq) !== next) return { rows: [], rowCount: 0 };
        events.push({
          trajectory_id: trajectoryId,
          seq: Number(seq),
          type,
          time,
          data: JSON.parse(data),
          ignorable,
        });
        return { rows: [], rowCount: 1 };
      }

      if (text.includes("count(*)::int AS events")) {
        const [id] = values as string[];
        return {
          rows: [{ events: events.filter((e) => e.trajectory_id === id).length }],
          rowCount: 1,
        };
      }

      if (text.includes("FROM foundry.trajectory t")) {
        const [id] = values as string[];
        const row = headers.find((h) => h.id === id);
        return { rows: row ? [row] : [], rowCount: row ? 1 : 0 };
      }

      if (text.includes("FROM foundry.trajectory_event")) {
        const [id, through] = values as [string, number | null];
        const rows = events
          .filter((e) => e.trajectory_id === id)
          .filter((e) => through === null || e.seq <= through)
          .sort((a, b) => a.seq - b.seq);
        return { rows, rowCount: rows.length };
      }

      throw new Error(`unexpected query: ${text}`);
    }
  }

  return { db: { query } as unknown as FoundryDb, headers, events };
}

const HEADER: Omit<Trajectory, "createdAt"> = {
  id: "traj-1",
  sceneId: "flagtag",
  provenance: "bot",
  runner: "arena",
  finishReason: null,
  parentTrajectoryId: null,
  seedLength: null,
  evidencePath: null,
};

function evt(
  seq: number,
  type: TrajectoryEvent["type"],
  data: unknown,
  time = "2026-08-14T23:29:52.000Z",
): Omit<TrajectoryEvent, "trajectoryId"> {
  return { seq, type, time, data };
}

async function seeded() {
  const fake = fakeDb();
  await createTrajectory(fake.db, HEADER);
  await appendEvents(fake.db, HEADER.id, [
    evt(0, "turn/start", { turn: 1 }, "2026-08-14T23:29:52.000Z"),
    evt(1, "tool/call", { callId: "c1", name: "dclbots.arena", arguments: "{}" }),
    evt(2, "obs/snapshot", { stream: "stdout", line: "flagtag sandbox: seed 7" }),
    evt(3, "check/verdict", { kind: "exit-code", pass: true, detail: "exit 0", why: "rc 0" }),
    evt(
      4,
      "turn/end",
      { turn: 1, reason: { kind: "completed" } },
      "2026-08-14T23:29:52.313Z",
    ),
  ]);
  return fake;
}

describe("trajectory event log", () => {
  it("appends contiguous events and reads them back in order", async () => {
    const fake = await seeded();
    const record = await getTrajectory(fake.db, "traj-1");
    expect(record?.events.map((e) => e.seq)).toEqual([0, 1, 2, 3, 4]);
    expect(record?.events[2].data).toEqual({
      stream: "stdout",
      line: "flagtag sandbox: seed 7",
    });
  });

  it("refuses a gap in seq", async () => {
    const fake = await seeded();
    await expect(
      appendEvents(fake.db, "traj-1", [evt(9, "obs/snapshot", { line: "later" })]),
    ).rejects.toBeInstanceOf(TrajectorySeqError);
  });

  it("refuses data that is not lossless JSON", async () => {
    const fake = await seeded();
    await expect(
      appendEvents(fake.db, "traj-1", [
        evt(5, "obs/snapshot", { at: new Date("2026-08-14T00:00:00Z") }),
      ]),
    ).rejects.toBeInstanceOf(TrajectoryLogError);
  });

  it("refuses an unknown event type that is not marked ignorable", async () => {
    const fake = await seeded();
    await expect(
      appendEvents(fake.db, "traj-1", [
        { seq: 5, type: "vibes/check" as TrajectoryEvent["type"], time: "2026-08-14T23:29:53.000Z", data: {} },
      ]),
    ).rejects.toBeInstanceOf(TrajectoryLogError);
  });

  it("derives the finish reason from the last turn/end", async () => {
    const fake = await seeded();
    const record = await getTrajectory(fake.db, "traj-1");
    expect(deriveFinishReason(record?.events ?? [])).toEqual({ kind: "completed" });
    expect(deriveFinishReason([])).toBeNull();
  });
});

describe("forkTrajectory", () => {
  it("copies the prefix, records lineage and closes the seed", async () => {
    const fake = await seeded();
    const childId = await forkTrajectory(fake.db, "traj-1", 4);
    const child = await getTrajectory(fake.db, childId);

    expect(child?.header.parentTrajectoryId).toBe("traj-1");
    expect(child?.header.seedLength).toBe(5);
    expect(child?.events.map((e) => e.type)).toEqual([
      "turn/start",
      "tool/call",
      "obs/snapshot",
      "check/verdict",
      "turn/end",
      "run/end-seed",
    ]);
    const source = await getTrajectory(fake.db, "traj-1");
    expect(source?.events).toHaveLength(5);
  });

  it("refuses a boundary inside an open turn", async () => {
    const fake = await seeded();
    await expect(forkTrajectory(fake.db, "traj-1", 2)).rejects.toBeInstanceOf(
      TrajectoryForkError,
    );
  });

  it("refuses a boundary past the end of the log", async () => {
    const fake = await seeded();
    await expect(forkTrajectory(fake.db, "traj-1", 9)).rejects.toBeInstanceOf(
      TrajectoryForkError,
    );
  });
});
