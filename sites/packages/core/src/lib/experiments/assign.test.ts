import { describe, expect, it } from "vitest";

import type { StoryMeta } from "./context";
import {
  bucket,
  createSid,
  cyrb53,
  ensureSid,
  expireSidCookie,
  hashToUnitInterval,
  hasSplitSidCookie,
  readSid,
  resolveAssignment,
  serializeSidCookie,
  sharedSidDomain,
} from "./assign";

function makeStory(
  variants: { id: string; weight: number; flags?: Record<string, unknown> }[],
  key = "test-experiment",
): StoryMeta {
  return {
    id: "test",
    status: "running",
    owner: "qa",
    hypothesis: { statement: "x", because: "y" },
    metric: { primary: "conversion", guardrails: [] },
    experiment: {
      key,
      unit: "session",
      variants: variants.map((v) => ({
        id: v.id,
        weight: v.weight,
        flags: v.flags ?? {},
      })),
    },
    decision: { rule: "ship if primary up" },
  };
}

function requestWithSid(sid: string): Request {
  return new Request("https://sites.example.com/", {
    headers: { cookie: `sid=${sid}` },
  });
}

describe("sid cookie helpers", () => {
  it("reads an existing sid and reports it as not-created", () => {
    const req = requestWithSid("abc-123");
    expect(readSid(req)).toBe("abc-123");
    expect(ensureSid(req)).toEqual({ sid: "abc-123", created: false });
  });

  it("creates a stable sid when none is present", () => {
    const req = new Request("https://sites.example.com/");
    expect(readSid(req)).toBeNull();
    const { sid, created } = ensureSid(req);
    expect(created).toBe(true);
    expect(sid.length).toBeGreaterThan(0);
  });

  it("generates unique-ish ids", () => {
    const ids = new Set(Array.from({ length: 100 }, () => createSid()));
    expect(ids.size).toBe(100);
  });

  it("serializes a safe Set-Cookie", () => {
    const cookie = serializeSidCookie("s-1");
    expect(cookie).toContain("sid=s-1");
    expect(cookie).toContain("Path=/");
    expect(cookie).toContain("HttpOnly");
    expect(cookie).toContain("SameSite=Lax");
    expect(cookie).toContain("Secure");
  });
});

describe("sid cookie convergence helpers", () => {
  it("expireSidCookie mirrors the sid attributes with an empty value and Max-Age=0", () => {
    const host = expireSidCookie();
    expect(host.startsWith("sid=;")).toBe(true);
    expect(host).toContain("Path=/");
    expect(host).toContain("Max-Age=0");
    expect(host).toContain("SameSite=Lax");
    expect(host).toContain("HttpOnly");
    expect(host).toContain("Secure");
    expect(host).not.toContain("Domain=");
    expect(expireSidCookie({ domain: "catalyst.example.com" })).toContain("Domain=catalyst.example.com");
  });

  it("sharedSidDomain widens only hosts under the shared parent", () => {
    const at = (url: string) => sharedSidDomain(new Request(url));
    expect(at("https://studio.catalyst.example.com/studio")).toBe("catalyst.example.com");
    expect(at("https://catalyst.example.com/")).toBe("catalyst.example.com");
    expect(at("https://sites.example.com/")).toBeUndefined();
    // A suffix match that is not a subdomain must never widen.
    expect(at("https://evilcatalyst.example.com/")).toBeUndefined();
  });

  it("hasSplitSidCookie detects two same-named sid cookies and nothing else", () => {
    const mk = (cookie: string) =>
      new Request("https://studio.catalyst.example.com/", { headers: { cookie } });
    expect(hasSplitSidCookie(mk("sid=a; sid=b"))).toBe(true);
    expect(hasSplitSidCookie(mk("sid=a; other=b"))).toBe(false);
    expect(hasSplitSidCookie(new Request("https://studio.catalyst.example.com/"))).toBe(false);
  });
});

describe("deterministic hashing", () => {
  it("cyrb53 is stable for the same input", () => {
    expect(cyrb53("hello")).toBe(cyrb53("hello"));
    expect(cyrb53("hello")).not.toBe(cyrb53("world"));
  });

  it("hashToUnitInterval stays within [0,1)", () => {
    for (let i = 0; i < 1000; i++) {
      const u = hashToUnitInterval(`sid-${i}:key`);
      expect(u).toBeGreaterThanOrEqual(0);
      expect(u).toBeLessThan(1);
    }
  });
});

describe("bucket() determinism", () => {
  const story = makeStory([
    { id: "control", weight: 0.5 },
    { id: "treatment", weight: 0.5 },
  ]);

  it("same sid + key -> same variant, every time", () => {
    const sids = ["alice", "bob", "carol", "owner@example.com", "ZZZ-999"];
    for (const sid of sids) {
      const first = bucket(sid, story.experiment.key, story.experiment.variants);
      for (let i = 0; i < 20; i++) {
        expect(
          bucket(sid, story.experiment.key, story.experiment.variants).id,
        ).toBe(first.id);
      }
    }
  });

  it("different experiment keys can yield different arms for one sid", () => {
    let differ = 0;
    for (let i = 0; i < 200; i++) {
      const sid = `sid-${i}`;
      const a = bucket(sid, "exp-a", story.experiment.variants).id;
      const b = bucket(sid, "exp-b", story.experiment.variants).id;
      if (a !== b) differ++;
    }
    expect(differ).toBeGreaterThan(0);
  });

  it("single-variant story always returns that variant", () => {
    const solo = makeStory([{ id: "only", weight: 1 }]);
    expect(bucket("x", solo.experiment.key, solo.experiment.variants).id).toBe(
      "only",
    );
  });
});

describe("bucket() weight distribution", () => {
  it("~respects a 50/50 split over many sids", () => {
    const story = makeStory([
      { id: "control", weight: 0.5 },
      { id: "treatment", weight: 0.5 },
    ]);
    const counts: Record<string, number> = { control: 0, treatment: 0 };
    const N = 20000;
    for (let i = 0; i < N; i++) {
      const v = bucket(`sid-${i}`, story.experiment.key, story.experiment.variants);
      counts[v.id]++;
    }
    expect(counts.control / N).toBeGreaterThan(0.46);
    expect(counts.control / N).toBeLessThan(0.54);
    expect(counts.control + counts.treatment).toBe(N);
  });

  it("~respects an unequal 70/20/10 split (weights need not sum to 1)", () => {
    const story = makeStory([
      { id: "a", weight: 70 },
      { id: "b", weight: 20 },
      { id: "c", weight: 10 },
    ]);
    const counts: Record<string, number> = { a: 0, b: 0, c: 0 };
    const N = 30000;
    for (let i = 0; i < N; i++) {
      counts[
        bucket(`u-${i}`, story.experiment.key, story.experiment.variants).id
      ]++;
    }
    expect(counts.a / N).toBeGreaterThan(0.66);
    expect(counts.a / N).toBeLessThan(0.74);
    expect(counts.b / N).toBeGreaterThan(0.16);
    expect(counts.b / N).toBeLessThan(0.24);
    expect(counts.c / N).toBeGreaterThan(0.07);
    expect(counts.c / N).toBeLessThan(0.13);
  });

  it("treats all-zero weights as an even split", () => {
    const story = makeStory([
      { id: "a", weight: 0 },
      { id: "b", weight: 0 },
    ]);
    const counts: Record<string, number> = { a: 0, b: 0 };
    const N = 10000;
    for (let i = 0; i < N; i++) {
      counts[bucket(`z-${i}`, story.experiment.key, story.experiment.variants).id]++;
    }
    expect(counts.a / N).toBeGreaterThan(0.45);
    expect(counts.a / N).toBeLessThan(0.55);
  });
});

describe("resolveAssignment (no backends configured)", () => {
  it("falls back to the deterministic local hash and is repeatable", async () => {
    const story = makeStory([
      { id: "control", weight: 0.5, flags: { hero: "a" } },
      { id: "treatment", weight: 0.5, flags: { hero: "b" } },
    ]);
    const sid = "stable-session-id";
    const a1 = await resolveAssignment(requestWithSid(sid), story);
    const a2 = await resolveAssignment(requestWithSid(sid), story);
    expect(a1).toEqual(a2);
    expect(a1.experimentKey).toBe(story.experiment.key);
    expect(["control", "treatment"]).toContain(a1.variant);
    expect(a1.flags).toHaveProperty("hero");
  });

  it("matches bucket() for the same sid (local-hash layer is authoritative)", async () => {
    const story = makeStory([
      { id: "control", weight: 0.5 },
      { id: "treatment", weight: 0.5 },
    ]);
    const sid = "another-session";
    const assignment = await resolveAssignment(requestWithSid(sid), story);
    const expected = bucket(sid, story.experiment.key, story.experiment.variants);
    expect(assignment.variant).toBe(expected.id);
  });

  it("always returns a valid variant even with an empty cookie", async () => {
    const story = makeStory([
      { id: "control", weight: 1 },
      { id: "treatment", weight: 1 },
    ]);
    const req = new Request("https://sites.example.com/");
    const assignment = await resolveAssignment(req, story);
    expect(["control", "treatment"]).toContain(assignment.variant);
  });
});
