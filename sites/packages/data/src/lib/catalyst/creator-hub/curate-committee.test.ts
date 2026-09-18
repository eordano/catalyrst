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
  it("the fixture validates, exposes a committee with a connected member, and carries ForumNewPost-faithful comment threads", () => {
    expect(CommitteeFixtureSchema.safeParse(fixture).success).toBe(true);
    const committee = fixtureCommittee();
    expect(committee.you.address).toMatch(/^0x/);
    expect(
      committee.members.some(
        (m) => m.address.toLowerCase() === committee.you.address.toLowerCase(),
      ),
    ).toBe(true);
    const withThread = fixtureRows().find((r) => r.comments.length > 0);
    expect(withThread).toBeDefined();
    const c = withThread!.comments[0];
    expect(c.raw.length).toBeGreaterThan(0);
    expect(c.author).toMatch(/^0x/);
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
  it("normalise status and type to known filters and map me/you to the connected wallet", () => {
    const you = "0x9F3C4D1E7A2188CF90B3A6E7C4D5F6A7B8C9D0E1";
    expect(readStatusFilter("to_review")).toBe("to_review");
    expect(readStatusFilter("bogus")).toBe("ALL_STATUS");
    expect(readTypeFilter("third_party")).toBe("third_party");
    expect(readTypeFilter("")).toBe("ALL_TYPES");
    expect(readAssigneeFilter("me", you)).toBe(you.toLowerCase());
    expect(readAssigneeFilter("all", you)).toBe("all");
  });
});

describe("filterRows", () => {
  it("?status=to_review keeps only To review rows and ALL filters return every row", () => {
    const rows = fixtureRows();
    const out = filterRows(rows, { status: "to_review", type: "ALL_TYPES", assignee: "all" });
    expect(out.length).toBeGreaterThan(0);
    for (const r of out) expect(deriveDisplayState(r)).toBe("to_review");
    expect(
      filterRows(rows, { status: "ALL_STATUS", type: "ALL_TYPES", assignee: "all" }).length,
    ).toBe(rows.length);
  });
});

describe("row projection and optimistic comments", () => {
  it("toBdRow carries assignee, you flag, comments and forumTopicId; buildOptimisticComment mirrors the would-be ForumNewPost", () => {
    const committee = fixtureCommittee();
    const row = fixtureRows().find((r) => r.name === "Neon Streetwear Drop") as CommitteeRow;
    const bd = toBdRow(row, committee);
    expect(bd.assignee).toBe(committee.you.address);
    expect(bd.you).toBe(true);
    expect(bd.assigneeName).toBe(committee.you.name);
    expect(bd.comments.length).toBe(row.comments.length);
    expect(bd.forumTopicId).toBe(row.forumTopicId);

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
