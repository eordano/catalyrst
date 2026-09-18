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

function distribution(story: StoryMeta, prefix: string, n: number): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const v of story.experiment.variants) counts[v.id] = 0;
  for (let i = 0; i < n; i++) {
    counts[bucket(`${prefix}-${i}`, story.experiment.key, story.experiment.variants).id]++;
  }
  return counts;
}

describe("sid cookie helpers", () => {
  it("reads an existing sid as not-created, mints a unique one otherwise", () => {
    const req = requestWithSid("abc-123");
    expect(readSid(req)).toBe("abc-123");
    expect(ensureSid(req)).toEqual({ sid: "abc-123", created: false });
    const bare = new Request("https://sites.example.com/");
    expect(readSid(bare)).toBeNull();
    const { sid, created } = ensureSid(bare);
    expect(created).toBe(true);
    expect(sid.length).toBeGreaterThan(0);
    expect(new Set(Array.from({ length: 100 }, () => createSid())).size).toBe(100);
  });

  it("serializes a safe Set-Cookie and expireSidCookie mirrors its attributes with Max-Age=0", () => {
    const cookie = serializeSidCookie("s-1");
    expect(cookie).toContain("sid=s-1");
    expect(cookie).toContain("Path=/");
    expect(cookie).toContain("HttpOnly");
    expect(cookie).toContain("SameSite=Lax");
    expect(cookie).toContain("Secure");
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

  it("sharedSidDomain widens only hosts under the shared parent; hasSplitSidCookie detects duplicate sids only", () => {
    const at = (url: string) => sharedSidDomain(new Request(url));
    expect(at("https://studio.catalyst.example.com/studio")).toBe("catalyst.example.com");
    expect(at("https://catalyst.example.com/")).toBe("catalyst.example.com");
    expect(at("https://sites.example.com/")).toBeUndefined();
    expect(at("https://evilcatalyst.example.com/")).toBeUndefined();
    const mk = (cookie: string) => new Request("https://studio.catalyst.example.com/", { headers: { cookie } });
    expect(hasSplitSidCookie(mk("sid=a; sid=b"))).toBe(true);
    expect(hasSplitSidCookie(mk("sid=a; other=b"))).toBe(false);
    expect(hasSplitSidCookie(new Request("https://studio.catalyst.example.com/"))).toBe(false);
  });
});

describe("bucket()", () => {
  const story = makeStory([
    { id: "control", weight: 0.5 },
    { id: "treatment", weight: 0.5 },
  ]);

  it("hashing is stable and unit-bounded", () => {
    expect(cyrb53("hello")).toBe(cyrb53("hello"));
    expect(cyrb53("hello")).not.toBe(cyrb53("world"));
    for (let i = 0; i < 1000; i++) {
      const u = hashToUnitInterval(`sid-${i}:key`);
      expect(u).toBeGreaterThanOrEqual(0);
      expect(u).toBeLessThan(1);
    }
  });

  it("is deterministic per (sid, key), keys differ, and a single variant always wins", () => {
    for (const sid of ["alice", "bob", "carol", "owner@example.com", "ZZZ-999"]) {
      const first = bucket(sid, story.experiment.key, story.experiment.variants);
      for (let i = 0; i < 20; i++) {
        expect(bucket(sid, story.experiment.key, story.experiment.variants).id).toBe(first.id);
      }
    }
    let differ = 0;
    for (let i = 0; i < 200; i++) {
      const a = bucket(`sid-${i}`, "exp-a", story.experiment.variants).id;
      const b = bucket(`sid-${i}`, "exp-b", story.experiment.variants).id;
      if (a !== b) differ++;
    }
    expect(differ).toBeGreaterThan(0);
    const solo = makeStory([{ id: "only", weight: 1 }]);
    expect(bucket("x", solo.experiment.key, solo.experiment.variants).id).toBe("only");
  });

  it("respects 50/50, unnormalized 70/20/10, and all-zero weights as an even split", () => {
    const even = distribution(story, "sid", 20000);
    expect(even.control / 20000).toBeGreaterThan(0.46);
    expect(even.control / 20000).toBeLessThan(0.54);
    expect(even.control + even.treatment).toBe(20000);

    const skewed = distribution(
      makeStory([
        { id: "a", weight: 70 },
        { id: "b", weight: 20 },
        { id: "c", weight: 10 },
      ]),
      "u",
      30000,
    );
    expect(skewed.a / 30000).toBeGreaterThan(0.66);
    expect(skewed.a / 30000).toBeLessThan(0.74);
    expect(skewed.b / 30000).toBeGreaterThan(0.16);
    expect(skewed.b / 30000).toBeLessThan(0.24);
    expect(skewed.c / 30000).toBeGreaterThan(0.07);
    expect(skewed.c / 30000).toBeLessThan(0.13);

    const zero = distribution(
      makeStory([
        { id: "a", weight: 0 },
        { id: "b", weight: 0 },
      ]),
      "z",
      10000,
    );
    expect(zero.a / 10000).toBeGreaterThan(0.45);
    expect(zero.a / 10000).toBeLessThan(0.55);
  });
});

describe("resolveAssignment (no backends configured)", () => {
  it("falls back to the repeatable local hash that matches bucket(), even with an empty cookie", async () => {
    const story = makeStory([
      { id: "control", weight: 0.5, flags: { hero: "a" } },
      { id: "treatment", weight: 0.5, flags: { hero: "b" } },
    ]);
    const sid = "stable-session-id";
    const a1 = await resolveAssignment(requestWithSid(sid), story);
    const a2 = await resolveAssignment(requestWithSid(sid), story);
    expect(a1).toEqual(a2);
    expect(a1.experimentKey).toBe(story.experiment.key);
    expect(a1.variant).toBe(bucket(sid, story.experiment.key, story.experiment.variants).id);
    expect(a1.flags).toHaveProperty("hero");
    const bare = await resolveAssignment(new Request("https://sites.example.com/"), story);
    expect(["control", "treatment"]).toContain(bare.variant);
  });
});
