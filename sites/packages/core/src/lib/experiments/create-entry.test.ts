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
  it("all story dirs parse; mapped ones carry their key, and two-arm ones are control + own arm", () => {
    const all = fs
      .readdirSync(STORIES_ROOT, { withFileTypes: true })
      .filter((d) => d.isDirectory())
      .map((d) => d.name);
    expect(all.length).toBeGreaterThanOrEqual(dirs.length);
    for (const dir of all) {
      expect(() => parseStory(path.join(STORIES_ROOT, dir))).not.toThrow();
    }
    for (const dir of dirs) {
      const meta = parseStory(path.join(STORIES_ROOT, dir));
      expect(meta.experiment.key).toBe(CREATE_ENTRY_STORIES[dir]);
      expect(meta.experiment.unit).toBe("session");
      if (dir === "entry-preview") continue;
      expect(meta.experiment.variants.map((v) => v.id)).toEqual(["control", dir]);
      expect(entryFromFlags(meta.experiment.variants[1].flags)).toBe(dir);
      expect(entryFromFlags(meta.experiment.variants[0].flags)).toBeNull();
    }
    expect(entryFromFlags({})).toBeNull();
    expect(entryFromFlags({ entry: 7 })).toBeNull();
    expect(entryFromFlags({ entry: "not-an-arm" })).toBeNull();
  });

  it("entry-preview multi-arm covers all four treatments + control", () => {
    const meta = parseStory(path.join(STORIES_ROOT, "entry-preview"));
    expect(new Set(meta.experiment.variants.map((v) => v.id))).toEqual(
      new Set(["control", ...dirs.filter((d) => d !== "entry-preview")]),
    );
    for (const v of meta.experiment.variants) {
      expect(entryFromFlags(v.flags)).toBe(v.id === "control" ? null : v.id);
    }
    const cap = meta.experiment.variants.find((v) => v.id === "capability-routed")!;
    expect(webHubIfCapable(cap.flags)).toBe(true);
    expect(webHubIfCapable(meta.experiment.variants[0].flags)).toBe(false);
  });
});

describe("activeCreateExperiment (CREATE_EXPERIMENT env)", () => {
  it("accepts a story dir name or an experiment key and rejects unset, non-entry or unknown values", () => {
    expect(activeCreateExperiment("entry-preview")).toBe("entry-preview");
    expect(activeCreateExperiment("  capability-routed ")).toBe("capability-routed");
    expect(activeCreateExperiment("create_download_hub")).toBe("download-hub");
    expect(activeCreateExperiment("create_entry_preview")).toBe("entry-preview");
    expect(activeCreateExperiment(undefined)).toBeNull();
    expect(activeCreateExperiment(null)).toBeNull();
    expect(activeCreateExperiment("")).toBeNull();
    expect(activeCreateExperiment("hub-to-scenes")).toBeNull();
    expect(activeCreateExperiment("templates-gallery")).toBeNull();
    expect(activeCreateExperiment("nonsense")).toBeNull();
  });
});

describe("armOverride (?arm=)", () => {
  it("returns a matching variant id and ignores missing or unknown arms", () => {
    const variants = [{ id: "control" }, { id: "download-hub" }];
    expect(armOverride(new URL("https://x/create?arm=download-hub"), variants)).toBe(
      "download-hub",
    );
    expect(armOverride(new URL("https://x/create?arm=control"), variants)).toBe("control");
    expect(armOverride(new URL("https://x/create"), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/create?arm="), variants)).toBeUndefined();
    expect(armOverride(new URL("https://x/create?arm=bogus"), variants)).toBeUndefined();
  });
});

describe("CTA targets", () => {
  it("point at real routes", () => {
    expect(CREATE_ENTRY_TARGETS.builder).toMatch(/^\/create\/wearables\/item-editor/);
    expect(CREATE_ENTRY_TARGETS.webHub).toMatch(/^\/creator-hub\/scene-editor/);
    expect(CREATE_ENTRY_TARGETS.download).toBe("/landings/creator-hub-download");
  });
});
