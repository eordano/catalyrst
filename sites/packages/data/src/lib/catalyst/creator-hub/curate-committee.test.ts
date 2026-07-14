import { describe, expect, it } from "vitest";

import fixture from "../../../fixtures/creator-curate-committee.json";
import {
  CommitteeFixtureSchema,
  buildOptimisticComment,
  deriveDisplayState,
  filterRows,
  readAssigneeFilter,
  readStatusFilter,
  readTypeFilter,
  toBdRow,
  type CommitteeRow,
} from "./curate-committee";

const parsedFixture = CommitteeFixtureSchema.parse(fixture);
const fixtureCommittee = () => parsedFixture.committee;
const fixtureRows = (): CommitteeRow[] => parsedFixture.collections;

describe("fixture / schema", () => {
  it("the fixture validates against the zod contract", () => {
    expect(CommitteeFixtureSchema.safeParse(fixture).success).toBe(true);
  });

  it("exposes a committee + a connected (you) member", () => {
    const committee = fixtureCommittee();
    expect(committee.you.address).toMatch(/^0x/);
    expect(
      committee.members.some(
        (m) => m.address.toLowerCase() === committee.you.address.toLowerCase(),
      ),
    ).toBe(true);
  });

  it("carries CurationComment threads faithful to ForumNewPost { raw, topic_id }", () => {
    const withThread = fixtureRows().find((r) => r.comments.length > 0);
    expect(withThread).toBeDefined();
    const c = withThread!.comments[0];
    expect(typeof c.raw).toBe("string");
    expect(c.raw.length).toBeGreaterThan(0);
    expect(c.author).toMatch(/^0x/);
    expect([null, "approved", "rejected"]).toContain(c.decision);
  });
});

describe("display-state derivation (reused from builder-curation)", () => {
  it("the fixture rows derive the expected display states", () => {
    const byName = Object.fromEntries(
      fixtureRows().map((r) => [r.name, deriveDisplayState(r)]),
    );
    expect(byName["Genesis Threads"]).toBe("to_review");
    expect(byName["Neon Streetwear Drop"]).toBe("under_review");
    expect(byName["Cyber Samurai Armory"]).toBe("approved");
    expect(byName["Lo-Fi Emote Pack"]).toBe("rejected");
    expect(byName["Desert Festival Wearables"]).toBe("disabled");
  });
});

describe("URL filter readers", () => {
  const you = "0x9F3C4D1E7A2188CF90B3A6E7C4D5F6A7B8C9D0E1";

  it("readStatusFilter normalises to a known CurationStatusFilter", () => {
    expect(readStatusFilter("to_review")).toBe("to_review");
    expect(readStatusFilter("bogus")).toBe("ALL_STATUS");
  });

  it("readTypeFilter normalises to a known type", () => {
    expect(readTypeFilter("third_party")).toBe("third_party");
    expect(readTypeFilter("")).toBe("ALL_TYPES");
  });

  it("readAssigneeFilter maps me/you to the connected wallet", () => {
    expect(readAssigneeFilter("me", you)).toBe(you.toLowerCase());
    expect(readAssigneeFilter("all", you)).toBe("all");
  });
});

describe("filterRows", () => {
  const rows = fixtureRows();

  it("?status=to_review keeps only To review rows", () => {
    const out = filterRows(rows, { status: "to_review", type: "ALL_TYPES", assignee: "all" });
    expect(out.length).toBeGreaterThan(0);
    for (const r of out) expect(deriveDisplayState(r)).toBe("to_review");
  });

  it("ALL filters return every row", () => {
    const out = filterRows(rows, { status: "ALL_STATUS", type: "ALL_TYPES", assignee: "all" });
    expect(out.length).toBe(rows.length);
  });
});

describe("toBdRow projection (carries the comment thread)", () => {
  const committee = fixtureCommittee();

  it("projects assignee name + you flag + comments + forumTopicId", () => {
    const row = fixtureRows().find((r) => r.name === "Neon Streetwear Drop") as CommitteeRow;
    const bd = toBdRow(row, committee);
    expect(bd.assignee).toBe(committee.you.address);
    expect(bd.you).toBe(true);
    expect(bd.assigneeName).toBe(committee.you.name);
    expect(bd.comments.length).toBe(row.comments.length);
    expect(bd.forumTopicId).toBe(row.forumTopicId);
  });
});

describe("buildOptimisticComment (mirrors the would-be ForumNewPost)", () => {
  const committee = fixtureCommittee();

  it("builds a comment tagged with the decision + author", () => {
    const c = buildOptimisticComment({
      collectionId: "0x1f2e3d4c5b6a7980a1b2c3d4e5f60718293a4b5c",
      author: committee.you,
      decision: "rejected",
      raw: "Needs original animations.",
      topicId: 50121,
      now: Date.parse("2026-06-24T10:00:00.000Z"),
    });
    expect(c.author).toBe(committee.you.address);
    expect(c.authorName).toBe(committee.you.name);
    expect(c.decision).toBe("rejected");
    expect(c.raw).toBe("Needs original animations.");
    expect(c.topic_id).toBe(50121);
    expect(c.created_at).toBe("2026-06-24T10:00:00.000Z");
  });
});
