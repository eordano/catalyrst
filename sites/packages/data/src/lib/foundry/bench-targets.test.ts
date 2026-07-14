import { describe, expect, it } from "vitest";

import {
  benchTargetsSentence,
  tallyBenchTargets,
} from "@ui/foundry/pages/FdBenchPage";

// The bench/copilot surfaces word their target sentence from rows, and these
// tests pin the wording at the honest extremes so the sentence can never
// silently overclaim: local-copy bench runs must never read as runs against
// the deployed Worlds.

const LOCAL = { sceneId: "flagtag", realm: "http://127.0.0.1:8000" };
const LOCAL2 = { sceneId: "skychaser", realm: "localhost:8000" };
const WORLD = { sceneId: "flagtag", realm: "flagtag.dcl.eth" };

describe("tallyBenchTargets", () => {
  it("classifies loopback realms as local, others as world, null as unrecorded", () => {
    expect(tallyBenchTargets([LOCAL, LOCAL2, WORLD, { sceneId: "x", realm: null }]))
      .toEqual({ world: 1, local: 2, unrecorded: 1 });
  });

  it("counts scenes, not runs: two realms on one scene stay one scene per class", () => {
    expect(
      tallyBenchTargets([LOCAL, { sceneId: "flagtag", realm: "127.0.0.1:9000" }]),
    ).toEqual({ world: 0, local: 1, unrecorded: 0 });
  });
});

describe("benchTargetsSentence", () => {
  it("admits when nothing has run", () => {
    expect(benchTargetsSentence([])).toBe("no bot bench run has been ingested yet");
  });

  it("says local copies — and only local copies — when no run touched a World", () => {
    const s = benchTargetsSentence([LOCAL, LOCAL2]);
    expect(s).toBe(
      "bench runs so far exercised local copies of 2 scenes, not the deployed Worlds",
    );
  });

  it("moves the day a run targets a deployed World", () => {
    expect(benchTargetsSentence([WORLD])).toBe(
      "1 scene has bench runs against its deployed World",
    );
    expect(benchTargetsSentence([WORLD, LOCAL2])).toBe(
      "1 scene has bench runs against its deployed World, and 1 against local copies",
    );
  });

  it("admits when the harness recorded no targets", () => {
    expect(benchTargetsSentence([{ sceneId: "x", realm: null }])).toBe(
      "bench runs are recorded, but the harness did not record their targets",
    );
  });
});
