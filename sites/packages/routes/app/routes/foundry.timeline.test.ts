import { describe, expect, it, vi } from "vitest";

/*
 * The cursor contract: ?before is a keyset cursor that reaches SQL as a
 * timestamptz. An unparsable value must fall back to the top of the lane —
 * the same shrug a bogus ?lane gets — never reach the query and 500.
 */

const state = vi.hoisted(() => ({
  listCalls: [] as { before?: string | null }[],
}));

vi.mock("@data/lib/foundry/memory.server", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@data/lib/foundry/memory.server")>();
  return {
    ...actual,
    listTimeline: vi.fn(async (opts: { before?: string | null } = {}) => {
      state.listCalls.push(opts);
      return { rows: [], nextBefore: null };
    }),
    timelineStats: vi.fn(async () => ({
      events: 0,
      actors: 0,
      firstMemory: null,
    })),
  };
});

import { eventHref, loader } from "./foundry.timeline";
import type { TimelineRow } from "@data/lib/foundry/types";

function get(search = "") {
  return {
    request: new Request(`https://sites.test/foundry/timeline${search}`),
    params: {},
    context: {} as never,
  };
}

describe("GET /foundry/timeline?before=", () => {
  it("falls back to the top of the lane on an unparsable cursor — 200, not 500", async () => {
    const res = await loader(get("?before=not-a-date") as never);
    expect(res.init?.status ?? 200).toBe(200);
    expect(res.data.before).toBeNull();
    expect(state.listCalls.at(-1)).toMatchObject({ before: null });
  });

  it("passes a parsable cursor through to the query and the page", async () => {
    const cursor = "2026-08-01T00:00:00.000Z";
    const res = await loader(get(`?before=${encodeURIComponent(cursor)}`) as never);
    expect(res.data.before).toBe(cursor);
    expect(state.listCalls.at(-1)).toMatchObject({ before: cursor });
  });
});

/*
 * eventHref: memory-derived rows (worlds' scene_changelog, community's
 * scene-subject action_log rows) link to their own /foundry/timeline/<id>
 * permalink; every other lane — including scene_note actions, whose note the
 * changelog arm already carries — keeps its existing subjectHref instead.
 */
function row(overrides: Partial<TimelineRow>): TimelineRow {
  return {
    lane: "community",
    id: "al-1",
    at: "2026-08-01T00:00:00.000Z",
    actor: { badge: "abcd" },
    action: "claim_steward",
    subject: "scene-1",
    subjectLabel: null,
    subjectKind: "scene",
    body: "",
    provenance: "visitor",
    runner: null,
    dateOnly: false,
    ...overrides,
  };
}

describe("eventHref", () => {
  it("links a worlds-lane changelog row to its c<id> permalink", () => {
    expect(eventHref(row({ lane: "worlds", id: "cl-42" }))).toBe(
      "/foundry/timeline/c42",
    );
  });

  it("links a community scene-action row to its a<id> permalink", () => {
    expect(eventHref(row({ lane: "community", id: "al-7", subjectKind: "scene", action: "claim_steward" }))).toBe(
      "/foundry/timeline/a7",
    );
  });

  it("does not link a scene_note row — its note already lives on the changelog row", () => {
    expect(eventHref(row({ lane: "community", id: "al-9", subjectKind: "scene", action: "scene_note" }))).toBeNull();
  });

  it("does not link community rows outside a scene subject", () => {
    expect(eventHref(row({ lane: "community", id: "al-3", subjectKind: "session", action: "rsvp_session" }))).toBeNull();
  });

  it("does not link other lanes — they keep their own detail-page subjectHref", () => {
    expect(eventHref(row({ lane: "exchange", id: "rq-2" }))).toBeNull();
    expect(eventHref(row({ lane: "trajectory", id: "tr-5" }))).toBeNull();
  });
});
