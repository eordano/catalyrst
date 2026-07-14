import path from "node:path";

import { describe, it, expect } from "vitest";

import { bucket } from "../../packages/core/src/lib/experiments/assign";
import { parseStory } from "../../packages/core/src/lib/experiments/context";

const STORY_DIR = path.join(process.cwd(), "packages", "features", "src", "stories", "create/entry-preview");
const PREVIEW = parseStory(STORY_DIR);
const PREVIEW_KEY = "create_entry_preview";

describe("/create assignment (deterministic bucketing, no DB)", () => {
  it("spreads 5000 sids ~evenly across the 5 multi-arm arms (17-23% each)", () => {
    const counts = new Map<string, number>();
    const N = 5000;
    for (let i = 0; i < N; i++) {
      const v = bucket(`sid-dist-${i}`, PREVIEW_KEY, PREVIEW.experiment.variants);
      counts.set(v.id, (counts.get(v.id) ?? 0) + 1);
    }
    expect(PREVIEW.experiment.variants.length).toBe(5);
    for (const variant of PREVIEW.experiment.variants) {
      const pct = (counts.get(variant.id) ?? 0) / N;
      expect(pct).toBeGreaterThan(0.17);
      expect(pct).toBeLessThan(0.23);
    }
  });

  it("maps the fixed per-arm sids the render/readout tests reuse", () => {
    const arm = (sid: string) =>
      bucket(sid, PREVIEW_KEY, PREVIEW.experiment.variants).id;
    expect(arm("e2e-sid-0")).toBe("control");
    expect(arm("e2e-sid-1")).toBe("capability-routed");
    expect(arm("e2e-sid-2")).toBe("download-hub");
    expect(arm("e2e-sid-4")).toBe("builder-or-download");
    expect(arm("e2e-sid-7")).toBe("hub-or-download");
  });
});
