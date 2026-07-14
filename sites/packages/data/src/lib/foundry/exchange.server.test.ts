import { describe, expect, it, vi } from "vitest";

import {
  getRequest,
  isDateOnly,
  listRequests,
  validateRequest,
} from "./exchange.server";

// The board and detail queries answer from this stub pool — the queries
// themselves (join set included) are what these tests pin down.
const queries: { sql: string; values: unknown[] }[] = [];
let stubRows: unknown[] = [];

vi.mock("./db.server", async (importOriginal) => {
  const mod = await importOriginal<typeof import("./db.server")>();
  return {
    ...mod,
    getPool: () => ({
      query: async (sql: string, values: unknown[]) => {
        queries.push({ sql, values });
        return { rows: stubRows };
      },
    }),
  };
});

function dbRow(overrides: Record<string, unknown> = {}) {
  return {
    id: "ask-0123456789abcdef",
    title: "Bring back parkour weekends",
    body: "A body",
    source: "u/someone",
    status: "open",
    sid: null,
    author_name: null,
    mod_action: null,
    mod_at: null,
    mod_sid: null,
    mod_name: null,
    pledges: 2,
    pledged_by_me: false,
    authored_by_me: false,
    origin: "import",
    created_at: new Date("2026-08-15T00:00:00Z"),
    edited_at: null,
    source_url: "https://example.com/ask",
    sourced_at: null,
    reading_cell: "creator-led-social-competition",
    reading_read_at: "2026-08-17",
    ...overrides,
  };
}

// Precision is a stored fact set by the importer: a fixture date with no clock
// component lands as sourced_date_only=true, which is what the timeline reads
// instead of sniffing midnights out of the stored timestamp.
describe("isDateOnly", () => {
  it("recognises a bare day", () => {
    expect(isDateOnly("2026-05-27")).toBe(true);
  });

  it("keeps a real instant timed — midnight included", () => {
    expect(isDateOnly("2026-05-27T00:00:00Z")).toBe(false);
    expect(isDateOnly("2026-05-27T14:03:00Z")).toBe(false);
  });
});

describe("the board's reading column", () => {
  it("a read ask carries {cell, readAt}; a null-cell reading is still a reading; no row is null", async () => {
    stubRows = [
      dbRow({}),
      dbRow({ id: "ask-1111111111111111", reading_cell: null }),
      dbRow({
        id: "rq-visitor",
        origin: "visitor",
        reading_cell: null,
        reading_read_at: null,
      }),
    ];
    const [read, fitsNone, unread] = await listRequests("sid-a");
    expect(read?.reading).toEqual({
      cell: "creator-led-social-competition",
      readAt: "2026-08-17",
    });
    // read_at is NOT NULL, so its presence alone says a reading row exists.
    expect(fitsNone?.reading).toEqual({ cell: null, readAt: "2026-08-17" });
    expect(unread?.reading).toBeNull();
  });

  it("the detail query inherits the reading join, and both answer in one shape", async () => {
    queries.length = 0;
    stubRows = [dbRow({})];
    const detail = await getRequest("sid-a", "ask-0123456789abcdef");
    const detailSql = queries.at(-1)?.sql ?? "";
    expect(detailSql).toContain("foundry.request_reading");
    expect(detailSql).toContain("WHERE r.id = $2");

    const [board] = await listRequests("sid-a");
    const boardSql = queries.at(-1)?.sql ?? "";
    expect(boardSql).toContain("foundry.request_reading");
    // One mapper: the detail row IS the board row, byte for byte.
    expect(detail).toEqual(board);
  });
});

describe("the board's authorship and edit columns", () => {
  it("maps authored_by_me and edited_at, and the SQL resolves the viewer's principal set", async () => {
    queries.length = 0;
    stubRows = [
      dbRow({
        id: "rq-mine",
        origin: "visitor",
        sid: "sid-a",
        authored_by_me: true,
        edited_at: new Date("2026-08-22T10:00:00Z"),
      }),
      dbRow({}),
    ];
    const [mine, imported] = await listRequests("sid-a");
    expect(mine?.authoredByMe).toBe(true);
    expect(mine?.editedAt).toBe("2026-08-22T10:00:00.000Z");
    expect(imported?.authoredByMe).toBe(false);
    expect(imported?.editedAt).toBeNull();
    // Ownership goes through the alias layer — the raw-sid comparison bug
    // (finding 11's shape) would drop the persona_sid resolution.
    const sql = queries.at(-1)?.sql ?? "";
    expect(sql).toContain("AS authored_by_me");
    expect(sql).toMatch(/COALESCE\(aal\.persona_sid, r\.sid\) IN/);
    expect(sql).toContain("WITH canon AS");
  });
});

describe("validateRequest", () => {
  it("lets the provenance field stay empty — a first-person ask is not an error", () => {
    expect(
      validateRequest({ title: "A title", body: "A body", source: "" }),
    ).toEqual({});
  });

  it("still bounds a provided source", () => {
    expect(
      validateRequest({ title: "A title", body: "A body", source: "s".repeat(61) }),
    ).toHaveProperty("source");
  });
});
