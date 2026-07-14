import { beforeEach, describe, expect, it, vi } from "vitest";

import { listTimeline, timelineStats } from "./memory.server";

// The merged memory is one union query, so the fake below is a dispatcher, not
// a database: each test loads the rows the union would have returned and the
// assertions cover what the reader DERIVES from them — verbs, subject labels,
// subject kinds, actor resolution — plus the SQL the reader actually sends.
type FakeRow = Record<string, unknown>;

const state = vi.hoisted(() => ({
  rows: [] as FakeRow[],
  statsRow: null as FakeRow | null,
  personas: [] as { sid: string; display_name: string }[],
  queries: [] as string[],
}));

vi.mock("./db.server", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./db.server")>();
  async function query(
    text: string,
    _values: unknown[] = [],
  ): Promise<{ rows: unknown[]; rowCount: number }> {
    state.queries.push(text);
    if (text.includes("JOIN foundry.persona")) {
      return { rows: state.personas, rowCount: state.personas.length };
    }
    if (text.includes("AS first_memory")) {
      const rows = state.statsRow ? [state.statsRow] : [];
      return { rows, rowCount: rows.length };
    }
    return { rows: state.rows, rowCount: state.rows.length };
  }
  return { ...actual, getPool: () => ({ query }) };
});

function row(overrides: FakeRow): FakeRow {
  return {
    lane: "community",
    id: "al-1",
    at: "2026-08-15T12:00:00.000Z",
    sid: null,
    action: "post_request",
    subject: null,
    subject_label: null,
    body: "",
    provenance: "visitor",
    runner: null,
    source_url: null,
    subject_kind: null,
    ...overrides,
  };
}

beforeEach(() => {
  state.rows = [];
  state.statsRow = null;
  state.personas = [];
  state.queries = [];
});

describe("listTimeline — community subjects", () => {
  it("renders label-free verbs so the resolved subject label can link beside them", async () => {
    state.rows = [
      row({
        id: "al-1",
        sid: "sid-a",
        action: "post_request",
        subject: "rq-9",
        subject_label: "A parkour map",
        subject_kind: "request",
      }),
      row({
        id: "al-2",
        sid: "sid-a",
        action: "claim_steward",
        subject: "flagtag",
        subject_label: "Flag Tag",
        subject_kind: "scene",
      }),
      row({
        id: "al-3",
        sid: "sid-a",
        action: "rsvp_session",
        subject: "ss-1",
        subject_label: "Friday playtest",
        subject_kind: "session",
      }),
    ];
    const { rows } = await listTimeline();
    expect(rows.map((r) => r.body)).toEqual([
      "posted a request",
      "claimed stewardship of a scene",
      "said they'll come to a session",
    ]);
    expect(rows.map((r) => r.subjectKind)).toEqual(["request", "scene", "session"]);
    for (const r of rows) {
      expect(r.subjectLabel).not.toBeNull();
      expect(r.body).not.toContain(r.subjectLabel as string);
    }
  });

  it("keeps a dangling subject nameless — verb only, no invented label", async () => {
    state.rows = [
      row({
        sid: "sid-a",
        action: "rsvp_session",
        subject: "ss-gone",
        subject_label: null,
        subject_kind: "session",
      }),
    ];
    const { rows } = await listTimeline();
    expect(rows[0].body).toBe("said they'll come to a session");
    expect(rows[0].subjectLabel).toBeNull();
    expect(rows[0].subjectKind).toBe("session");
  });

  it("resolves community subjects from their own tables in SQL", async () => {
    await listTimeline({ lanes: ["community"] });
    const union = state.queries[0];
    expect(union).toContain("LEFT JOIN foundry.scene sc");
    expect(union).toContain("LEFT JOIN foundry.session_series ss");
    expect(union).toContain("AS subject_kind");
  });

  it("resolves doc subjects for approval/edit/publish actions from gdd_doc", async () => {
    await listTimeline({ lanes: ["community"] });
    const union = state.queries[0];
    expect(union).toContain("('approve_gdd','edit_gdd_doc','publish_gdd_draft')");
    expect(union).toContain("THEN gd.title");
    expect(union).toContain("LEFT JOIN foundry.gdd_doc gd ON gd.id = a.subject");
    expect(union).toContain("THEN 'doc'");
  });

  it("keeps session plumbing off the feed — carry mints, redeems and rebinds are the holder's business", async () => {
    await listTimeline({ lanes: ["community"] });
    const union = state.queries[0];
    expect(union).toContain(
      "a.action NOT IN ('mint_carry_code','redeem_carry_code','rebind_grant')",
    );
  });

  it("gives only request actions the request kind — no fallback that would link a persona sid", async () => {
    await listTimeline({ lanes: ["community"] });
    const union = state.queries[0];
    expect(union).toContain(
      "('post_request','edit_request','pledge','withdraw_pledge','approve_request','close_request')",
    );
    expect(union).not.toContain("ELSE 'request'");
    expect(union).not.toContain("ELSE r.title");
  });

  it("ships a null subject kind for a persona claim, so no href is derived", async () => {
    state.rows = [
      row({
        sid: "sid-a",
        action: "claim_persona",
        subject: "sid-a",
        subject_label: null,
        subject_kind: null,
      }),
    ];
    const { rows } = await listTimeline();
    expect(rows[0].body).toBe("claimed a persona");
    expect(rows[0].subjectKind).toBeNull();
    expect(rows[0].subjectLabel).toBeNull();
  });
});

describe("listTimeline — docs lane", () => {
  it("moves the doc title into the subject label and keeps a label-free verb", async () => {
    state.rows = [
      row({
        lane: "docs",
        id: "gd-7",
        action: "design_doc",
        subject: "flagtag",
        subject_label: "Flag Tag — shortGDD",
        provenance: "import",
      }),
    ];
    const { rows } = await listTimeline();
    expect(rows[0].body).toBe("design doc imported");
    expect(rows[0].subjectLabel).toBe("Flag Tag — shortGDD");
    expect(rows[0].actor).toEqual({ source: "design workspace" });
  });

  it("attributes a program-drafted doc to the program itself — never 'imported'", async () => {
    state.rows = [
      row({
        lane: "docs",
        id: "gd-8",
        action: "design_doc",
        subject_label: "Open ground: community-operated game clubs",
        provenance: "recorded",
        runner: "program",
      }),
    ];
    const { rows } = await listTimeline();
    expect(rows[0].body).toBe("drafted a design doc");
    expect(rows[0].actor).toEqual({ source: "this program" });
    expect(rows[0].provenance).toBe("recorded");
  });
});

describe("timelineStats", () => {
  it("dates the first memory from the instant the exchange lane renders — the imported ask's original public date", async () => {
    state.statsRow = {
      events: 12,
      actors: 3,
      first_memory: new Date("2021-03-16T00:00:00.000Z"),
    };
    const stats = await timelineStats();
    expect(stats).toEqual({
      events: 12,
      actors: 3,
      firstMemory: "2021-03-16T00:00:00.000Z",
    });
    const sql = state.queries[0];
    expect(sql).toContain("min(COALESCE(sourced_at, created_at))");
    expect(sql).not.toMatch(/min\(created_at\) FROM foundry\.request/);
  });

  it("returns measured zeros and a null first memory when nothing is recorded", async () => {
    state.statsRow = { events: 0, actors: 0, first_memory: null };
    const stats = await timelineStats();
    expect(stats).toEqual({ events: 0, actors: 0, firstMemory: null });
  });
});
