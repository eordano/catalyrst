import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  EDIT_IMPORTED,
  EDIT_NOT_AUTHOR,
  editRequest,
} from "./exchange.server";

// Enough of Postgres to exercise the ownership contract in memory: requests
// answer from a fixed registry, canonical-sid resolution walks an alias map,
// and the assertions cover the boundary — only the author (through any of
// their persona's sids) edits, an imported ask stays verbatim, and the prior
// wording is recorded before the overwrite.
const state = vi.hoisted(() => ({
  requests: new Map<
    string,
    {
      title: string;
      body: string;
      origin: string;
      sid: string | null;
      edited_at: string | null;
    }
  >(),
  aliases: new Map<string, string>(),
  actions: [] as {
    sid: string;
    action: string;
    subject: string;
    detail: Record<string, unknown>;
  }[],
}));

vi.mock("./db.server", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./db.server")>();
  const s = state;
  async function query(sql: string, values: unknown[] = []) {
    if (/SELECT title, body, origin, sid FROM foundry\.request/.test(sql)) {
      const row = s.requests.get(values[0] as string);
      return row ? { rowCount: 1, rows: [row] } : { rowCount: 0, rows: [] };
    }
    if (/SELECT COALESCE\(/.test(sql)) {
      const sid = values[0] as string;
      return { rowCount: 1, rows: [{ sid: s.aliases.get(sid) ?? sid }] };
    }
    if (/UPDATE foundry\.request SET title/.test(sql)) {
      const [id, title, body] = values as [string, string, string];
      const row = s.requests.get(id)!;
      s.requests.set(id, {
        ...row,
        title,
        body,
        edited_at: "2026-08-22T00:00:00Z",
      });
      return { rowCount: 1, rows: [] };
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
    withTx: async (fn: (c: unknown) => Promise<unknown>) => fn(client),
  };
});

describe("editRequest", () => {
  beforeEach(() => {
    state.requests.clear();
    state.aliases.clear();
    state.actions.length = 0;
    state.requests.set("rq-1", {
      title: "Old title",
      body: "Old body",
      origin: "visitor",
      sid: "sid-author",
      edited_at: null,
    });
    state.requests.set("ask-import", {
      title: "Quoted title",
      body: "A quoted body",
      origin: "import",
      sid: null,
      edited_at: null,
    });
  });

  it("lets the author revise wording, stamps edited_at, and logs the prior text", async () => {
    await editRequest({
      requestId: "rq-1",
      title: "New title",
      body: "New body",
      sid: "sid-author",
    });
    const row = state.requests.get("rq-1")!;
    expect(row.title).toBe("New title");
    expect(row.body).toBe("New body");
    expect(row.edited_at).not.toBeNull();
    expect(state.actions).toEqual([
      {
        sid: "sid-author",
        action: "edit_request",
        subject: "rq-1",
        detail: { prev_title: "Old title", prev_body: "Old body" },
      },
    ]);
  });

  it("refuses a stranger, changing nothing", async () => {
    await expect(
      editRequest({
        requestId: "rq-1",
        title: "Hijacked",
        body: "Hijacked body",
        sid: "sid-stranger",
      }),
    ).rejects.toThrow(EDIT_NOT_AUTHOR);
    expect(state.requests.get("rq-1")!.title).toBe("Old title");
    expect(state.actions).toHaveLength(0);
  });

  it("refuses an imported ask — verbatim public speech", async () => {
    await expect(
      editRequest({
        requestId: "ask-import",
        title: "Rewritten",
        body: "Rewritten body",
        sid: "sid-author",
      }),
    ).rejects.toThrow(EDIT_IMPORTED);
    expect(state.requests.get("ask-import")!.body).toBe("A quoted body");
  });

  it("resolves ownership through the alias layer — the finding-11 regression trap", async () => {
    // The ask was authored from the canonical sid; the author now writes from
    // a rebound session whose live sid is an alias of it.
    state.aliases.set("sid-rebound", "sid-author");
    await editRequest({
      requestId: "rq-1",
      title: "Revised from the rebound session",
      body: "Still my ask.",
      sid: "sid-rebound",
    });
    expect(state.requests.get("rq-1")!.title).toBe(
      "Revised from the rebound session",
    );
  });

  it("refuses an edit that changes nothing", async () => {
    await expect(
      editRequest({
        requestId: "rq-1",
        title: "Old title",
        body: "Old body",
        sid: "sid-author",
      }),
    ).rejects.toThrow(/Nothing changed/);
    expect(state.requests.get("rq-1")!.edited_at).toBeNull();
  });

  it("bounds the fields before touching the row", async () => {
    await expect(
      editRequest({
        requestId: "rq-1",
        title: "t".repeat(81),
        body: "fine",
        sid: "sid-author",
      }),
    ).rejects.toThrow(/80 characters/);
    await expect(
      editRequest({
        requestId: "rq-1",
        title: "fine",
        body: "b".repeat(281),
        sid: "sid-author",
      }),
    ).rejects.toThrow(/280 characters/);
    expect(state.actions).toHaveLength(0);
  });

  it("refuses a request that does not exist", async () => {
    await expect(
      editRequest({
        requestId: "rq-ghost",
        title: "A title",
        body: "A body",
        sid: "sid-author",
      }),
    ).rejects.toThrow(/no longer exists/);
  });
});
