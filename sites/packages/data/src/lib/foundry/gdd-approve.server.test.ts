import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Pool } from "pg";

import {
  approvalCounts,
  approvalsForDoc,
  approveGddDoc,
  hasApproved,
} from "./gdd-approve.server";

// Enough of Postgres to exercise the signature contract in memory: docs and
// personas answer from fixed registries, sid aliasing walks an alias map (the
// canonical-sid and principal-set SQL), the approval insert honours the
// (doc, signer) unique index, and the assertions cover the boundary — a
// signature needs a claimed name, binds once PER PERSON, and is recorded in
// the log.
const state = vi.hoisted(() => ({
  docs: new Set<string>(),
  personas: new Map<string, string>(),
  aliases: new Map<string, string>(),
  approvals: [] as { id: number; doc_id: string; sid: string; at: string }[],
  actions: [] as { sid: string; action: string; subject: string }[],
  nextId: 1,
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
  async function query(sql: string, values: unknown[] = []) {
    if (/SELECT 1 FROM foundry\.gdd_doc/.test(sql)) {
      return { rowCount: s.docs.has(values[0] as string) ? 1 : 0, rows: [] };
    }
    // CANONICAL_SQL starts the statement; the principal-set SQL embeds the
    // same COALESCE deeper in, so the anchor keeps the two apart.
    if (/^SELECT COALESCE\(/.test(sql)) {
      return { rowCount: 1, rows: [{ sid: canonical(values[0] as string) }] };
    }
    if (/SELECT display_name FROM foundry\.persona/.test(sql)) {
      const name = s.personas.get(values[0] as string);
      return name
        ? { rowCount: 1, rows: [{ display_name: name }] }
        : { rowCount: 0, rows: [] };
    }
    if (/INSERT INTO foundry\.gdd_approval/.test(sql)) {
      const [doc_id, sid] = values as [string, string];
      if (s.approvals.some((a) => a.doc_id === doc_id && a.sid === sid)) {
        return { rowCount: 0, rows: [] };
      }
      const row = { id: s.nextId++, doc_id, sid, at: `2026-08-19T0${s.nextId}:00:00Z` };
      s.approvals.push(row);
      return { rowCount: 1, rows: [{ at: row.at }] };
    }
    if (/INSERT INTO foundry\.action_log/.test(sql)) {
      s.actions.push({
        sid: values[0] as string,
        action: values[1] as string,
        subject: values[2] as string,
      });
      return { rowCount: 1, rows: [] };
    }
    if (/JOIN foundry\.persona p ON p\.sid = COALESCE\(al\.persona_sid, a\.sid\)/.test(sql)) {
      const rows = s.approvals
        .filter((a) => a.doc_id === values[0] && s.personas.has(a.sid))
        .map((a) => ({ display_name: s.personas.get(a.sid), at: a.at }));
      return { rowCount: rows.length, rows };
    }
    if (/GROUP BY doc_id/.test(sql)) {
      const ids = values[0] as string[];
      const rows = ids
        .map((id) => ({
          doc_id: id,
          n: s.approvals.filter((a) => a.doc_id === id).length,
        }))
        .filter((r) => r.n > 0);
      return { rowCount: rows.length, rows };
    }
    if (/SELECT 1 FROM foundry\.gdd_approval/.test(sql)) {
      // Both the write-path duplicate check and hasApproved: [sid, docId],
      // membership resolved across the persona's whole sid set.
      const set = principalSet(values[0] as string);
      const hit = s.approvals.some(
        (a) => a.doc_id === values[1] && set.has(a.sid),
      );
      return { rowCount: hit ? 1 : 0, rows: [] };
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

// approvalsForDoc/approvalCounts/hasApproved take a Pool — hand them the same
// dispatcher the mock installs as getPool().
import { getPool } from "./db.server";
const db = getPool() as unknown as Pool;

describe("approveGddDoc", () => {
  beforeEach(() => {
    state.docs.clear();
    state.personas.clear();
    state.aliases.clear();
    state.approvals.length = 0;
    state.actions.length = 0;
    state.nextId = 1;
    state.docs.add("zoo-v1");
    state.personas.set("sid-a", "Zap");
  });

  it("records a named signature once and logs approve_gdd", async () => {
    const res = await approveGddDoc({ docId: "zoo-v1", sid: "sid-a" });
    expect(res.name).toBe("Zap");
    expect(state.approvals).toHaveLength(1);
    expect(state.actions).toEqual([
      { sid: "sid-a", action: "approve_gdd", subject: "zoo-v1" },
    ]);
  });

  it("refuses without a claimed persona — a signature needs a name", async () => {
    await expect(
      approveGddDoc({ docId: "zoo-v1", sid: "sid-anon" }),
    ).rejects.toThrow(/signed with a name/);
    expect(state.approvals).toHaveLength(0);
  });

  it("refuses a second signature from the same session", async () => {
    await approveGddDoc({ docId: "zoo-v1", sid: "sid-a" });
    await expect(
      approveGddDoc({ docId: "zoo-v1", sid: "sid-a" }),
    ).rejects.toThrow(/already approved/);
    expect(state.approvals).toHaveLength(1);
  });

  it("refuses a doc that does not exist", async () => {
    await expect(
      approveGddDoc({ docId: "ghost-v9", sid: "sid-a" }),
    ).rejects.toThrow(/No such design document/);
  });

  it("signs from an aliased sid: named, landed canonically — the rosa/finding-30 shape", async () => {
    // The persona lives on sid-a; the live session was rebound onto sid-b.
    state.aliases.set("sid-b", "sid-a");
    const res = await approveGddDoc({ docId: "zoo-v1", sid: "sid-b" });
    expect(res.name).toBe("Zap");
    // The signature row lands on the canonical sid, not the alias.
    expect(state.approvals).toEqual([
      expect.objectContaining({ doc_id: "zoo-v1", sid: "sid-a" }),
    ]);
    // The log still attributes the acting session.
    expect(state.actions).toEqual([
      { sid: "sid-b", action: "approve_gdd", subject: "zoo-v1" },
    ]);
  });

  it("one person signs once, whichever of their sids asks", async () => {
    state.aliases.set("sid-b", "sid-a");
    await approveGddDoc({ docId: "zoo-v1", sid: "sid-a" });
    await expect(
      approveGddDoc({ docId: "zoo-v1", sid: "sid-b" }),
    ).rejects.toThrow(/already approved/);
    expect(state.approvals).toHaveLength(1);
    // hasApproved answers true from every sid of the persona, so the "You
    // approved" state survives a re-binding.
    expect(await hasApproved(db, "zoo-v1", "sid-a")).toBe(true);
    expect(await hasApproved(db, "zoo-v1", "sid-b")).toBe(true);
  });

  it("a pre-fix signature on an alias sid still blocks a canonical re-sign", async () => {
    state.aliases.set("sid-b", "sid-a");
    // Written before the canonical-landing fix: the row sits on the alias.
    state.approvals.push({
      id: state.nextId++,
      doc_id: "zoo-v1",
      sid: "sid-b",
      at: "2026-08-19T00:00:00Z",
    });
    await expect(
      approveGddDoc({ docId: "zoo-v1", sid: "sid-a" }),
    ).rejects.toThrow(/already approved/);
    expect(state.approvals).toHaveLength(1);
  });

  it("reads back signatures with resolved names, and counts per version", async () => {
    state.personas.set("sid-b", "Rowsdower");
    await approveGddDoc({ docId: "zoo-v1", sid: "sid-a" });
    await approveGddDoc({ docId: "zoo-v1", sid: "sid-b" });
    const list = await approvalsForDoc(db, "zoo-v1");
    expect(list.map((a) => a.name)).toEqual(["Zap", "Rowsdower"]);
    const counts = await approvalCounts(db, ["zoo-v1", "other-v1"]);
    expect(counts.get("zoo-v1")).toBe(2);
    expect(counts.has("other-v1")).toBe(false);
    expect(await hasApproved(db, "zoo-v1", "sid-a")).toBe(true);
    expect(await hasApproved(db, "zoo-v1", "sid-zzz")).toBe(false);
  });
});
