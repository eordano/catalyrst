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

describe("landing stories \u{2014} content shape", () => {
  it("ships exactly six audience variations", () => {
    expect(LANDING_STORIES).toHaveLength(6);
  });

  it("has unique, non-empty, url-safe slugs", () => {
    const ids = LANDING_STORIES.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) {
      expect(id).toMatch(/^[a-z][a-z0-9-]*$/);
    }
  });

  it("covers both creators and users", () => {
    const kinds = new Set(LANDING_STORIES.map((s) => s.kind));
    expect(kinds.has("creator")).toBe(true);
    expect(kinds.has("user")).toBe(true);
  });

  it("every variation is fully authored (headline, subhead, beats, cta)", () => {
    for (const s of LANDING_STORIES) {
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

  it("LANDING_STORY_IDS mirrors the list", () => {
    expect(LANDING_STORY_IDS).toEqual(LANDING_STORIES.map((s) => s.id));
  });
});

describe("getLandingStory \u{2014} resolution + fallback", () => {
  it("returns the matching story for a known slug", () => {
    for (const s of LANDING_STORIES) {
      expect(getLandingStory(s.id).id).toBe(s.id);
    }
  });

  it("falls back to the default for unknown / missing slugs", () => {
    for (const bad of ["nope", "", undefined, null]) {
      expect(getLandingStory(bad).id).toBe(DEFAULT_LANDING_STORY_ID);
    }
  });

  it("the default slug is itself a real story", () => {
    expect(LANDING_STORY_IDS).toContain(DEFAULT_LANDING_STORY_ID);
  });
});

describe("resolveAudienceFromParams \u{2014} UTM/query mapping", () => {
  it("maps friendly aliases to canonical ids across the accepted keys", () => {
    expect(resolveAudienceFromParams(q("utm_content=creators"))).toBe("scenes");
    expect(resolveAudienceFromParams(q("utm_content=fashion"))).toBe("wearables");
    expect(resolveAudienceFromParams(q("utm_audience=teams"))).toBe("studios");
    expect(resolveAudienceFromParams(q("a=nocode"))).toBe("first-timers");
    expect(resolveAudienceFromParams(q("audience=gamers"))).toBe("players");
    expect(resolveAudienceFromParams(q("utm_term=holders"))).toBe("collectors");
  });

  it("accepts canonical ids directly and is case-insensitive", () => {
    expect(resolveAudienceFromParams(q("a=first-timers"))).toBe("first-timers");
    expect(resolveAudienceFromParams(q("utm_content=Wearables"))).toBe("wearables");
  });

  it("returns null when nothing matches (empty / unknown value / unknown key)", () => {
    expect(resolveAudienceFromParams(q(""))).toBeNull();
    expect(resolveAudienceFromParams(q("utm_content=zzz"))).toBeNull();
    expect(resolveAudienceFromParams(q("foo=creators"))).toBeNull();
  });

  it("honors key priority (a beats utm_content)", () => {
    expect(resolveAudienceFromParams(q("utm_content=players&a=studios"))).toBe("studios");
  });
});

describe("pickLandingStory \u{2014} utm / sticky / random", () => {
  it("a UTM/query audience wins and is tagged via=utm", () => {
    const r = pickLandingStory(q("utm_content=collectors"));
    expect(r.via).toBe("utm");
    expect(r.story.id).toBe("collectors");
  });

  it("sticky: same seed \u{2192} same story, tagged via=sticky", () => {
    const a = pickLandingStory(q(""), { seed: "sid-123" });
    const b = pickLandingStory(q(""), { seed: "sid-123" });
    expect(a.via).toBe("sticky");
    expect(a.story.id).toBe(b.story.id);
    expect(LANDING_STORY_IDS).toContain(a.story.id);
  });

  it("sticky spreads visitors across more than one variation", () => {
    const ids = new Set(
      Array.from(
        { length: 40 },
        (_, i) => pickLandingStory(q(""), { seed: `sid-${i}` }).story.id,
      ),
    );
    expect(ids.size).toBeGreaterThan(1);
  });

  it("random: the injected rng selects the index; via=random", () => {
    expect(pickLandingStory(q(""), { rng: () => 0 }).story.id).toBe(LANDING_STORIES[0].id);
    expect(pickLandingStory(q(""), { rng: () => 0.999 }).story.id).toBe(
      LANDING_STORIES[LANDING_STORIES.length - 1].id,
    );
  });

  it("UTM overrides the sticky seed", () => {
    const r = pickLandingStory(q("a=players"), { seed: "sid-xyz" });
    expect(r.via).toBe("utm");
    expect(r.story.id).toBe("players");
  });
});
