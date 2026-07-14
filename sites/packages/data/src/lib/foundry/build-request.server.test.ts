import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Pool } from "pg";

import {
  activeBuildForDoc,
  claimableBuilds,
  requestBuild,
  setBuildStatus,
} from "./build-request.server";

// Enough of Postgres to exercise the build-request contract in memory: the
// doc-chain walk answers from a supersedes map, the maker resolves through a
// fake action log + alias map, the insert honours the build_request_active_once
// partial index, and the CAS update moves exactly the row it named. The
// assertions cover the boundary -- maker-or-host may ask, one live build per
// chain, the broker is the only status writer, and every act is logged.

const ACTIVE = new Set(["queued", "building", "verifying"]);

type BuildRow = {
  id: number;
  doc_id: string;
  scene_slug: string;
  requested_by_sid: string;
  status: string;
  detail: string;
  created_at: string;
  updated_at: string;
};

const state = vi.hoisted(() => ({
  docs: new Map<string, { supersedes: string | null }>(),
  actionLog: [] as { id: number; sid: string; action: string; subject: string }[],
  personas: new Map<string, string>(),
  aliases: new Map<string, string>(),
  scenes: new Set<string>(),
  hosts: new Set<string>(),
  consents: new Set<string>(),
  builds: [] as BuildRow[],
  actions: [] as { sid: string; action: string; subject: string; detail: unknown }[],
  nextId: 1,
}));

vi.mock("./roles.server", () => ({
  hasRole: async (_c: unknown, sid: string, role: string) =>
    role === "host" && state.hosts.has(sid),
}));

vi.mock("./consent.server", () => ({
  consentActive: async (_c: unknown, sid: string) => state.consents.has(sid),
}));

vi.mock("./db.server", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./db.server")>();
  const s = state;
  const canonical = (sid: string) => s.aliases.get(sid) ?? sid;
  const principalSet = (sid: string) => {
    const canon = canonical(sid);
    const set = new Set([canon]);
    for (const [alias, persona] of s.aliases) {
      if (persona === canon) set.add(alias);
    }
    return set;
  };
  const chainOf = (id: string): string[] => {
    if (!s.docs.has(id)) return [];
    const ids = new Set([id]);
    let cur = s.docs.get(id)?.supersedes ?? null;
    while (cur && s.docs.has(cur) && !ids.has(cur)) {
      ids.add(cur);
      cur = s.docs.get(cur)?.supersedes ?? null;
    }
    let grew = true;
    while (grew) {
      grew = false;
      for (const [did, d] of s.docs) {
        if (d.supersedes && ids.has(d.supersedes) && !ids.has(did)) {
          ids.add(did);
          grew = true;
        }
      }
    }
    return [...ids];
  };
  const withName = (b: BuildRow) => ({
    ...b,
    display_name: s.personas.get(canonical(b.requested_by_sid)) ?? null,
  });
  async function query(sql: string, values: unknown[] = []) {
    if (/FROM foundry\.build_request b/.test(sql) && /JOIN chain c/.test(sql)) {
      const chain = new Set(chainOf(values[0] as string));
      const rows = s.builds
        .filter((b) => chain.has(b.doc_id))
        .sort((a, b) => b.id - a.id)
        .slice(0, 1)
        .map(withName);
      return { rowCount: rows.length, rows };
    }
    if (/WHERE b\.status = 'queued'/.test(sql)) {
      const rows = s.builds
        .filter((b) => b.status === "queued")
        .sort((a, b) => a.id - b.id)
        .slice(0, values[0] as number)
        .map(withName);
      return { rowCount: rows.length, rows };
    }
    if (/SELECT id FROM older UNION SELECT id FROM newer/.test(sql)) {
      const rows = chainOf(values[0] as string).map((id) => ({ id }));
      return { rowCount: rows.length, rows };
    }
    if (/'edit_gdd_doc','publish_gdd_draft'/.test(sql)) {
      const chain = new Set(values[1] as string[]);
      const latest = [...s.actionLog]
        .filter(
          (a) =>
            (a.action === "edit_gdd_doc" || a.action === "publish_gdd_draft") &&
            chain.has(a.subject),
        )
        .sort((a, b) => b.id - a.id)[0];
      const hit = latest && principalSet(values[0] as string).has(latest.sid);
      return { rowCount: hit ? 1 : 0, rows: hit ? [{ one: 1 }] : [] };
    }
    if (/SELECT 1 FROM foundry\.scene WHERE id = \$1/.test(sql)) {
      return { rowCount: s.scenes.has(values[0] as string) ? 1 : 0, rows: [] };
    }
    if (/scene_slug = \$1 AND status IN/.test(sql)) {
      const hit = s.builds.some(
        (b) => b.scene_slug === values[0] && ACTIVE.has(b.status),
      );
      return { rowCount: hit ? 1 : 0, rows: [] };
    }
    if (/doc_id = ANY\(\$1\) AND status IN/.test(sql)) {
      const chain = new Set(values[0] as string[]);
      const hit = s.builds.some((b) => chain.has(b.doc_id) && ACTIVE.has(b.status));
      return { rowCount: hit ? 1 : 0, rows: [] };
    }
    if (/^SELECT COALESCE\(/.test(sql)) {
      return { rowCount: 1, rows: [{ sid: canonical(values[0] as string) }] };
    }
    if (/INSERT INTO foundry\.build_request/.test(sql)) {
      const [doc_id, scene_slug, requested_by_sid] = values as [string, string, string];
      // build_request_active_once: one live row per doc id.
      if (s.builds.some((b) => b.doc_id === doc_id && ACTIVE.has(b.status))) {
        throw Object.assign(new Error("duplicate key"), { code: "23505" });
      }
      const id = s.nextId++;
      s.builds.push({
        id,
        doc_id,
        scene_slug,
        requested_by_sid,
        status: "queued",
        detail: "",
        created_at: `2026-08-22T00:0${id}:00.000Z`,
        updated_at: `2026-08-22T00:0${id}:00.000Z`,
      });
      return { rowCount: 1, rows: [{ id }] };
    }
    if (/UPDATE foundry\.build_request/.test(sql)) {
      const [to, detail, id, from] = values as [string, string, number, string];
      const row = s.builds.find((b) => b.id === id && b.status === from);
      if (!row) return { rowCount: 0, rows: [] };
      row.status = to;
      row.detail = detail;
      return {
        rowCount: 1,
        rows: [{ doc_id: row.doc_id, scene_slug: row.scene_slug }],
      };
    }
    if (/INSERT INTO foundry\.action_log/.test(sql)) {
      s.actions.push({
        sid: values[0] as string,
        action: values[1] as string,
        subject: values[2] as string,
        detail: JSON.parse(values[3] as string),
      });
      return { rowCount: 1, rows: [] };
    }
    throw new Error(`unexpected SQL: ${sql}`);
  }
  const client = { query };
  return {
    ...actual,
    assertRate: () => {},
    getPool: () => ({ query }),
    withTx: async (fn: (c: unknown) => Promise<unknown>) => fn(client),
  };
});

import { getPool } from "./db.server";
const db = getPool() as unknown as Pool;

beforeEach(() => {
  state.docs.clear();
  state.actionLog.length = 0;
  state.personas.clear();
  state.aliases.clear();
  state.scenes.clear();
  state.hosts.clear();
  state.consents.clear();
  state.builds.length = 0;
  state.actions.length = 0;
  state.nextId = 1;
  state.docs.set("zoo-v1", { supersedes: null });
  state.docs.set("zoo-v2", { supersedes: "zoo-v1" });
  state.actionLog.push({
    id: 1,
    sid: "sid-maker",
    action: "publish_gdd_draft",
    subject: "zoo-v1",
  });
  state.personas.set("sid-maker", "Zap");
});

describe("requestBuild", () => {
  it("lets the chain's maker request; the row queues on the canonical sid and the act is logged", async () => {
    const res = await requestBuild({
      docId: "zoo-v2",
      sceneSlug: "zoo-run",
      sid: "sid-maker",
    });
    expect(res.id).toBe(1);
    expect(state.builds).toEqual([
      expect.objectContaining({
        doc_id: "zoo-v2",
        scene_slug: "zoo-run",
        requested_by_sid: "sid-maker",
        status: "queued",
      }),
    ]);
    expect(state.actions).toEqual([
      {
        sid: "sid-maker",
        action: "request_build",
        subject: "zoo-v2",
        detail: { scene_slug: "zoo-run" },
      },
    ]);
  });

  it("recognises the maker from an aliased sid and lands the row canonically", async () => {
    state.aliases.set("sid-rebound", "sid-maker");
    await requestBuild({ docId: "zoo-v1", sceneSlug: "zoo-run", sid: "sid-rebound" });
    expect(state.builds[0].requested_by_sid).toBe("sid-maker");
  });

  it("refuses a stranger and says what to do next", async () => {
    await expect(
      requestBuild({ docId: "zoo-v1", sceneSlug: "zoo-run", sid: "sid-nobody" }),
    ).rejects.toThrow(/maker or a host/);
    expect(state.builds).toHaveLength(0);
    expect(state.actions).toHaveLength(0);
  });

  it("lets a host with live steward-code consent request; a host without consent is refused", async () => {
    state.hosts.add("sid-host");
    await expect(
      requestBuild({ docId: "zoo-v1", sceneSlug: "zoo-a", sid: "sid-host" }),
    ).rejects.toThrow(/maker or a host/);
    state.consents.add("sid-host");
    await requestBuild({ docId: "zoo-v1", sceneSlug: "zoo-a", sid: "sid-host" });
    expect(state.builds).toHaveLength(1);
  });

  it("refuses a second live request anywhere on the chain", async () => {
    await requestBuild({ docId: "zoo-v1", sceneSlug: "zoo-run", sid: "sid-maker" });
    await expect(
      requestBuild({ docId: "zoo-v2", sceneSlug: "zoo-other", sid: "sid-maker" }),
    ).rejects.toThrow(/already underway/);
    expect(state.builds).toHaveLength(1);
  });

  it("maps the partial unique index's 23505 to the same refusal on a lost race", async () => {
    // The pre-check misses (no live row visible), the index catches it.
    state.builds.push({
      id: state.nextId++,
      doc_id: "zoo-v1",
      scene_slug: "zoo-x",
      requested_by_sid: "sid-else",
      status: "queued",
      detail: "",
      created_at: "2026-08-22T00:00:00.000Z",
      updated_at: "2026-08-22T00:00:00.000Z",
    });
    // Blind the two .some pre-checks (slug-busy, chain-active); the third
    // .some -- the index simulation inside INSERT -- still sees the row.
    const spy = vi
      .spyOn(state.builds, "some")
      .mockReturnValueOnce(false)
      .mockReturnValueOnce(false);
    await expect(
      requestBuild({ docId: "zoo-v1", sceneSlug: "zoo-y", sid: "sid-maker" }),
    ).rejects.toThrow(/already underway/);
    spy.mockRestore();
  });

  it("refuses a slug the shelf holds, a slug another build holds, and a malformed one", async () => {
    state.scenes.add("taken");
    await expect(
      requestBuild({ docId: "zoo-v1", sceneSlug: "taken", sid: "sid-maker" }),
    ).rejects.toThrow(/already on the shelf/);

    state.docs.set("ant-v1", { supersedes: null });
    state.builds.push({
      id: state.nextId++,
      doc_id: "ant-v1",
      scene_slug: "busy-slug",
      requested_by_sid: "sid-else",
      status: "building",
      detail: "",
      created_at: "2026-08-22T00:00:00.000Z",
      updated_at: "2026-08-22T00:00:00.000Z",
    });
    await expect(
      requestBuild({ docId: "zoo-v1", sceneSlug: "busy-slug", sid: "sid-maker" }),
    ).rejects.toThrow(/already being built/);

    await expect(
      requestBuild({ docId: "zoo-v1", sceneSlug: "Bad Slug", sid: "sid-maker" }),
    ).rejects.toThrow(/lowercase/);
    await expect(
      requestBuild({ docId: "zoo-v1", sceneSlug: "", sid: "sid-maker" }),
    ).rejects.toThrow(/Give the build a scene id/);
    await expect(
      requestBuild({ docId: "zoo-v1", sceneSlug: "register", sid: "sid-maker" }),
    ).rejects.toThrow(/register/);
  });

  it("refuses a doc that does not exist", async () => {
    await expect(
      requestBuild({ docId: "ghost-v9", sceneSlug: "ghost", sid: "sid-maker" }),
    ).rejects.toThrow(/No such design document/);
  });

  it("allows a fresh request after a failed one \u{2014} no live row survives a failure", async () => {
    await requestBuild({ docId: "zoo-v1", sceneSlug: "zoo-run", sid: "sid-maker" });
    await setBuildStatus({ id: 1, from: "queued", to: "failed", detail: "no toolchain" });
    await requestBuild({ docId: "zoo-v1", sceneSlug: "zoo-run-2", sid: "sid-maker" });
    expect(state.builds.map((b) => b.status)).toEqual(["failed", "queued"]);
  });
});

describe("activeBuildForDoc / claimableBuilds", () => {
  it("returns the chain's newest request from any version's page, name resolved on read", async () => {
    await requestBuild({ docId: "zoo-v2", sceneSlug: "zoo-run", sid: "sid-maker" });
    const fromV1 = await activeBuildForDoc(db, "zoo-v1");
    expect(fromV1).toMatchObject({
      docId: "zoo-v2",
      sceneSlug: "zoo-run",
      status: "queued",
      requestedBy: { name: "Zap" },
    });
    expect(await activeBuildForDoc(db, "unrelated")).toBeNull();
  });

  it("shows the honest badge when the requester never claimed a persona", async () => {
    state.actionLog.push({
      id: 2,
      sid: "sid-anon",
      action: "edit_gdd_doc",
      subject: "zoo-v2",
    });
    await requestBuild({ docId: "zoo-v2", sceneSlug: "zoo-run", sid: "sid-anon" });
    const row = await activeBuildForDoc(db, "zoo-v2");
    expect(row?.requestedBy).toHaveProperty("badge");
  });

  it("lists queued rows FIFO and nothing else", async () => {
    state.docs.set("ant-v1", { supersedes: null });
    state.actionLog.push({
      id: 2,
      sid: "sid-maker",
      action: "publish_gdd_draft",
      subject: "ant-v1",
    });
    await requestBuild({ docId: "zoo-v1", sceneSlug: "zoo-run", sid: "sid-maker" });
    await requestBuild({ docId: "ant-v1", sceneSlug: "ant-run", sid: "sid-maker" });
    await setBuildStatus({ id: 1, from: "queued", to: "building", detail: "" });
    const rows = await claimableBuilds(db);
    expect(rows.map((r) => r.docId)).toEqual(["ant-v1"]);
  });
});

describe("setBuildStatus", () => {
  beforeEach(async () => {
    await requestBuild({ docId: "zoo-v1", sceneSlug: "zoo-run", sid: "sid-maker" });
    state.actions.length = 0;
  });

  it("walks the legal ledger and logs each step against the doc", async () => {
    await setBuildStatus({ id: 1, from: "queued", to: "building", detail: "compiling" });
    await setBuildStatus({ id: 1, from: "building", to: "verifying", detail: "re-running tests" });
    await setBuildStatus({ id: 1, from: "verifying", to: "landed", detail: "commit abc123, 14 tests green" });
    expect(state.builds[0]).toMatchObject({
      status: "landed",
      detail: "commit abc123, 14 tests green",
    });
    expect(state.actions).toEqual([
      expect.objectContaining({
        action: "build_status",
        subject: "zoo-v1",
        detail: { scene_slug: "zoo-run", status: "building", note: "compiling" },
      }),
      expect.objectContaining({
        detail: { scene_slug: "zoo-run", status: "verifying", note: "re-running tests" },
      }),
      expect.objectContaining({
        detail: {
          scene_slug: "zoo-run",
          status: "landed",
          note: "commit abc123, 14 tests green",
        },
      }),
    ]);
  });

  it("refuses an illegal edge before touching the row", async () => {
    await expect(
      setBuildStatus({ id: 1, from: "queued", to: "landed", detail: "" }),
    ).rejects.toThrow(/never moves queued\u2192landed/);
    expect(state.builds[0].status).toBe("queued");
    expect(state.actions).toHaveLength(0);
  });

  it("refuses a lost compare-and-set out loud", async () => {
    await setBuildStatus({ id: 1, from: "queued", to: "building", detail: "" });
    await expect(
      setBuildStatus({ id: 1, from: "queued", to: "building", detail: "" }),
    ).rejects.toThrow(/not 'queued' any more/);
  });
});
