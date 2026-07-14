import fs from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  CREATE_ENTRY_STORIES,
  CREATE_ENTRY_TARGETS,
  activeCreateExperiment,
  armOverride,
  entryFromFlags,
  webHubIfCapable,
  type CreateEntryStoryName,
} from "./create-entry";
import { parseStory } from "./context";

const STORIES_ROOT = path.join(process.cwd(), "packages", "features", "src", "stories", "create");

const dirs = Object.keys(CREATE_ENTRY_STORIES) as CreateEntryStoryName[];

describe("create entry stories (app/stories/create/*)", () => {
  it("every mapped story dir exists and parses", () => {
    for (const dir of dirs) {
      const meta = parseStory(path.join(STORIES_ROOT, dir));
      expect(meta.experiment.key).toBe(CREATE_ENTRY_STORIES[dir]);
      expect(meta.experiment.unit).toBe("session");
      expect(meta.metric.primary).toBe("create_preview_rate");
    }
  });

  it("all seven create story.md files parse (incl. non-entry stories)", () => {
    const all = fs
      .readdirSync(STORIES_ROOT, { withFileTypes: true })
      .filter((d) => d.isDirectory())
      .map((d) => d.name);
    expect(all.length).toBe(7);
    for (const dir of all) {
      expect(() => parseStory(path.join(STORIES_ROOT, dir))).not.toThrow();
    }
  });

  it("standalone two-arm stories are control + their own arm", () => {
    for (const dir of dirs) {
      if (dir === "entry-preview") continue;
      const meta = parseStory(path.join(STORIES_ROOT, dir));
      expect(meta.experiment.variants.map((v) => v.id)).toEqual(["control", dir]);
      const treatment = meta.experiment.variants[1];
      expect(entryFromFlags(treatment.flags)).toBe(dir);
      expect(entryFromFlags(meta.experiment.variants[0].flags)).toBeNull();
    }
  });

  it("entry-preview multi-arm covers all four treatments + control", () => {
    const meta = parseStory(path.join(STORIES_ROOT, "entry-preview"));
    const ids = meta.experiment.variants.map((v) => v.id);
    expect(ids).toEqual([
      "control",
      "download-hub",
      "builder-or-download",
      "hub-or-download",
      "capability-routed",
    ]);
    for (const v of meta.experiment.variants) {
      expect(entryFromFlags(v.flags)).toBe(v.id === "control" ? null : v.id);
    }
    const cap = meta.experiment.variants.find((v) => v.id === "capability-routed")!;
    expect(webHubIfCapable(cap.flags)).toBe(true);
    expect(webHubIfCapable(meta.experiment.variants[0].flags)).toBe(false);
  });
});

describe("activeCreateExperiment (CREATE_EXPERIMENT env)", () => {
  it("accepts a story dir name", () => {
    expect(activeCreateExperiment("entry-preview")).toBe("entry-preview");
    expect(activeCreateExperiment("  capability-routed ")).toBe("capability-routed");
  });

  it("accepts an experiment key", () => {
    expect(activeCreateExperiment("create_download_hub")).toBe("download-hub");
    expect(activeCreateExperiment("create_entry_preview")).toBe("entry-preview");
  });

  it("rejects unset/unknown values (no experiment runs)", () => {
    expect(activeCreateExperiment(undefined)).toBeNull();
    expect(activeCreateExperiment(null)).toBeNull();
    expect(activeCreateExperiment("")).toBeNull();
    expect(activeCreateExperiment("hub-to-scenes")).toBeNull();
    expect(activeCreateExperiment("templates-gallery")).toBeNull();
    expect(activeCreateExperiment("nonsense")).toBeNull();
  });
});

describe("armOverride (?arm=)", () => {
  const variants = [{ id: "control" }, { id: "download-hub" }];

  it("returns a matching variant id", () => {
    expect(armOverride(new URL("https://x/create?arm=download-hub"), variants)).toBe(
      "download-hub",
    );
    expect(armOverride(new URL("https://x/create?arm=control"), variants)).toBe("control");
  });

  it("ignores missing/unknown arms", () => {
    expect(armOverride(new URL("https://x/create"), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/create?arm="), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/create?arm=bogus"), variants)).toBeUndefined();
  });
});

describe("entryFromFlags", () => {
  it("rejects non-string / unknown entries", () => {
    expect(entryFromFlags({})).toBeNull();
    expect(entryFromFlags({ entry: 7 })).toBeNull();
    expect(entryFromFlags({ entry: "not-an-arm" })).toBeNull();
  });
});

describe("CTA targets", () => {
  it("point at real routes", () => {
    expect(CREATE_ENTRY_TARGETS.builder).toMatch(/^\/create\/wearables\/item-editor/);
    expect(CREATE_ENTRY_TARGETS.webHub).toMatch(/^\/creator-hub\/scene-editor/);
    expect(CREATE_ENTRY_TARGETS.download).toBe("/landings/creator-hub-download");
  });
});
