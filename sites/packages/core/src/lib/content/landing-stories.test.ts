import { describe, expect, it } from "vitest";

import {
  DEFAULT_LANDING_STORY_ID,
  getLandingStory,
  LANDING_STORIES,
  LANDING_STORY_IDS,
  pickLandingStory,
  resolveAudienceFromParams,
} from "./landing-stories";

const q = (s: string) => new URLSearchParams(s);

describe("landing stories content shape", () => {
  it("ships fully authored, uniquely slugged variations covering creators and users", () => {
    expect(LANDING_STORIES.length).toBeGreaterThan(0);
    const ids = LANDING_STORIES.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(LANDING_STORY_IDS).toEqual(ids);
    expect(LANDING_STORY_IDS).toContain(DEFAULT_LANDING_STORY_ID);
    const kinds = new Set(LANDING_STORIES.map((s) => s.kind));
    expect(kinds.has("creator")).toBe(true);
    expect(kinds.has("user")).toBe(true);
    for (const s of LANDING_STORIES) {
      expect(s.id).toMatch(/^[a-z][a-z0-9-]*$/);
      expect(s.audience.trim().length).toBeGreaterThan(0);
      expect(s.headline.trim().length).toBeGreaterThan(0);
      expect(s.subhead.trim().length).toBeGreaterThan(0);
      expect(s.beats.length).toBeGreaterThanOrEqual(2);
      for (const b of s.beats) {
        expect(b.title.trim().length).toBeGreaterThan(0);
        expect(b.body.trim().length).toBeGreaterThan(0);
        expect(b.cta.label.trim().length).toBeGreaterThan(0);
        expect(b.cta.href).toMatch(/^\/[a-z]/);
      }
      expect(s.cta.label.trim().length).toBeGreaterThan(0);
      expect(s.cta.href).toMatch(/^\/[a-z]/);
    }
  });
});

describe("getLandingStory", () => {
  it("resolves every known slug and falls back to the default for unknown or missing ones", () => {
    for (const s of LANDING_STORIES) {
      expect(getLandingStory(s.id).id).toBe(s.id);
    }
    for (const bad of ["nope", "", undefined, null]) {
      expect(getLandingStory(bad).id).toBe(DEFAULT_LANDING_STORY_ID);
    }
  });
});

describe("resolveAudienceFromParams", () => {
  it("maps aliases and canonical ids case-insensitively across the accepted keys, a beating utm_content", () => {
    expect(resolveAudienceFromParams(q("utm_content=creators"))).toBe("scenes");
    expect(resolveAudienceFromParams(q("utm_content=fashion"))).toBe("wearables");
    expect(resolveAudienceFromParams(q("utm_audience=teams"))).toBe("studios");
    expect(resolveAudienceFromParams(q("a=nocode"))).toBe("first-timers");
    expect(resolveAudienceFromParams(q("audience=gamers"))).toBe("players");
    expect(resolveAudienceFromParams(q("utm_term=holders"))).toBe("collectors");
    expect(resolveAudienceFromParams(q("a=first-timers"))).toBe("first-timers");
    expect(resolveAudienceFromParams(q("utm_content=Wearables"))).toBe("wearables");
    expect(resolveAudienceFromParams(q("utm_content=players&a=studios"))).toBe("studios");
  });

  it("returns null for empty, unknown value or unknown key", () => {
    expect(resolveAudienceFromParams(q(""))).toBeNull();
    expect(resolveAudienceFromParams(q("utm_content=zzz"))).toBeNull();
    expect(resolveAudienceFromParams(q("foo=creators"))).toBeNull();
  });
});

describe("pickLandingStory", () => {
  it("a UTM/query audience wins, even over a sticky seed, tagged via=utm", () => {
    const r = pickLandingStory(q("utm_content=collectors"));
    expect(r.via).toBe("utm");
    expect(r.story.id).toBe("collectors");
    const seeded = pickLandingStory(q("a=players"), { seed: "sid-xyz" });
    expect(seeded.via).toBe("utm");
    expect(seeded.story.id).toBe("players");
  });

  it("sticky seeds are stable yet spread across variations; the injected rng selects the index", () => {
    const a = pickLandingStory(q(""), { seed: "sid-123" });
    const b = pickLandingStory(q(""), { seed: "sid-123" });
    expect(a.via).toBe("sticky");
    expect(a.story.id).toBe(b.story.id);
    expect(LANDING_STORY_IDS).toContain(a.story.id);
    const ids = new Set(
      Array.from({ length: 40 }, (_, i) => pickLandingStory(q(""), { seed: `sid-${i}` }).story.id),
    );
    expect(ids.size).toBeGreaterThan(1);
    expect(pickLandingStory(q(""), { rng: () => 0 }).story.id).toBe(LANDING_STORIES[0].id);
    expect(pickLandingStory(q(""), { rng: () => 0.999 }).story.id).toBe(
      LANDING_STORIES[LANDING_STORIES.length - 1].id,
    );
  });
});
