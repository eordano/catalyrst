import { describe, expect, it } from "vitest";
import {
  DEFAULT_CADENCE_SECONDS,
  NO_VALUE,
  STALE_SAMPLE_FACTOR,
  STALE_SNAPSHOT_DAYS,
  badgeText,
  type Datum,
  type DatumState,
  disagree,
  formatDatum,
  formatReadStamp,
  isStale,
  live,
  noSample,
  noteLines,
  requiresNote,
  sampled,
  showable,
  snapshot,
  stateWord,
  tallyStates,
  unavailable,
  unbuilt,
} from "./datum";

const NOW = Date.parse("2026-07-31T14:03:22Z");
const iso = (offsetMs: number) => new Date(NOW - offsetMs).toISOString();

/** One of every state, built only through the constructors. */
const CASES: { state: DatumState; datum: Datum<number>; showable: boolean }[] = [
  {
    state: "live",
    datum: live(5, "GET worlds-content-server.decentraland.org/live-data", iso(0)),
    showable: true,
  },
  {
    state: "sampled",
    datum: sampled(
      2,
      "GET catalyst.example.com/presence/current/worlds",
      iso(2 * 60_000),
      DEFAULT_CADENCE_SECONDS,
      iso(0),
    ),
    showable: true,
  },
  {
    state: "snapshot",
    datum: snapshot(
      409,
      "GET creators-data/api/worlds/{w}/metrics",
      iso(86_400_000),
      "metabase",
      iso(0),
    ),
    showable: true,
  },
  {
    state: "no-sample",
    datum: noSample(
      "GET catalyst.example.com/presence/current/worlds",
      iso(2 * 60_000),
      "No sample: this world was not live at the last snapshot. Not the same as zero.",
    ),
    showable: false,
  },
  {
    state: "unavailable",
    datum: unavailable(
      "GET worlds-content-server.decentraland.org/live-data",
      503,
      "GET worlds-content-server.decentraland.org/live-data returned 503.",
    ),
    showable: false,
  },
  {
    state: "unbuilt",
    datum: unbuilt(
      "Sessions & retention",
      "/creators/me/scenes/stats 404s.",
      "the headcounts above are the whole picture.",
    ),
    showable: false,
  },
];

describe("when classifying a datum", () => {
  it.each(CASES)(
    "should mark $state as showable=$showable",
    ({ datum, showable: expected }) => {
      expect(showable(datum)).toBe(expected);
    },
  );

  it.each(CASES.filter((c) => !c.showable))(
    "should give $state no value key at runtime, so `?? 0` has nothing to reach for",
    ({ datum }) => {
      expect(Object.hasOwn(datum, "value")).toBe(false);
      expect((datum as Record<string, unknown>).value).toBeUndefined();
    },
  );

  it.each(CASES.filter((c) => !c.showable))(
    "should require an explanatory note for $state",
    ({ datum }) => {
      expect(requiresNote(datum)).toBe(true);
    },
  );

  it.each(CASES.filter((c) => c.showable))(
    "should not require a provenance note for $state",
    ({ datum }) => {
      expect(requiresNote(datum)).toBe(false);
    },
  );
});

describe("when rendering a datum's value", () => {
  it("should render the value for showable states", () => {
    expect(formatDatum(live(5, "GET /x"), (v) => `${v}`)).toBe("5");
    expect(formatDatum(live(0, "GET /x"), (v) => `${v}`)).toBe("0");
  });

  it("should render the no-value glyph \u{2014} never a zero \u{2014} for every other state", () => {
    for (const { datum, showable: canShow } of CASES) {
      if (canShow) continue;
      expect(formatDatum(datum, (v) => `${v}`)).toBe(NO_VALUE);
    }
  });
});

describe("when deciding whether a sample is stale", () => {
  const cadence = DEFAULT_CADENCE_SECONDS;
  const limit = cadence * STALE_SAMPLE_FACTOR * 1000;

  it("should not be stale at exactly the cadence boundary", () => {
    const d = sampled(2, "GET /presence", iso(limit), cadence);
    expect(isStale(d, NOW)).toBe(false);
  });

  it("should be stale one millisecond past the boundary", () => {
    const d = sampled(2, "GET /presence", iso(limit + 1), cadence);
    expect(isStale(d, NOW)).toBe(true);
    expect(stateWord(d, NOW)).toBe("Stale");
  });

  it("should never call a live reading stale \u{2014} staleness is undefined for a real-time read", () => {
    expect(isStale(live(5, "GET /live-data", iso(86_400_000 * 400)), NOW)).toBe(
      false,
    );
  });

  it("should apply the 30-day boundary to snapshots", () => {
    const day = 86_400_000;
    const fresh = snapshot(1, "GET /m", iso(STALE_SNAPSHOT_DAYS * day), "metabase");
    const old = snapshot(
      1,
      "GET /m",
      iso(STALE_SNAPSHOT_DAYS * day + 1),
      "metabase",
    );
    expect(isStale(fresh, NOW)).toBe(false);
    expect(isStale(old, NOW)).toBe(true);
  });

  it("should never report staleness for the value-less states", () => {
    for (const { datum, showable: canShow } of CASES) {
      if (canShow) continue;
      expect(isStale(datum, NOW)).toBe(false);
    }
  });
});

describe("when naming a state", () => {
  it.each([
    ["live", "Live"],
    ["sampled", "Sampled"],
    ["snapshot", "Snapshot"],
    ["no-sample", "No sample"],
    ["unavailable", "Unavailable"],
    ["unbuilt", "Not built"],
  ] as const)("should call %s %s", (state, word) => {
    const found = CASES.find((c) => c.state === state);
    expect(found).toBeDefined();
    expect(stateWord(found!.datum, NOW)).toBe(word);
  });

  it("should append the sample age to the badge text", () => {
    const d = sampled(2, "GET /presence", iso(2 * 60_000), DEFAULT_CADENCE_SECONDS);
    expect(badgeText(d, NOW)).toBe("Sampled \u{B7} 2m ago");
  });

  it("should append the export date to a snapshot badge", () => {
    const d = snapshot(1, "GET /m", "2026-07-10T00:00:00Z", "metabase");
    expect(badgeText(d, NOW)).toBe("Snapshot \u{B7} exported 10 Jul 2026");
  });
});

describe("when a snapshot is not a trusted export", () => {
  it("should throw rather than hand back a renderable datum", () => {
    expect(() =>
      snapshot(409, "GET /api/worlds/w/metrics", "2026-07-10T00:00:00Z", "fixture"),
    ).toThrow(/exportSource/);
  });

  it("should accept only metabase", () => {
    expect(() =>
      snapshot(409, "GET /api/worlds/w/metrics", "2026-07-10T00:00:00Z", "metabase"),
    ).not.toThrow();
  });
});

describe("when writing the provenance note", () => {
  it("should always end an unavailable note with the no-guess sentence", () => {
    const d = unavailable("GET host/live-data", 503, "GET host/live-data returned 503.");
    expect(noteLines(d, NOW).at(-1)).toContain(
      "Showing no value rather than a guess.",
    );
  });

  it("should not duplicate the no-guess sentence when the reason already carries it", () => {
    const reason = "GET host/live-data returned 503. Showing no value rather than a guess.";
    const lines = noteLines(unavailable("GET host/live-data", 503, reason), NOW);
    expect(lines.at(-1)).toBe(reason);
  });

  it("should warn that the sampler may have stopped once a sample is stale", () => {
    const d = sampled(2, "GET /presence", iso(60 * 60_000), DEFAULT_CADENCE_SECONDS);
    expect(noteLines(d, NOW).join(" ")).toContain("the sampler may have stopped");
  });

  it("should add the Today line to an unbuilt datum only when there is one", () => {
    expect(noteLines(unbuilt("Bots", "No such endpoint.", "run it locally."))).toEqual(
      ["No such endpoint.", "Today: run it locally."],
    );
    expect(noteLines(unbuilt("Bots", "No such endpoint."))).toEqual([
      "No such endpoint.",
    ]);
  });
});

describe("when two hosts disagree", () => {
  it("should report a disagreement only when both readings exist and differ", () => {
    const presence = sampled(2, "GET /presence/current/worlds", iso(0));
    const liveData = live(3, "GET /live-data", iso(0));
    expect(disagree(presence, liveData)).toBe(true);
    expect(disagree(presence, live(2, "GET /live-data", iso(0)))).toBe(false);
    expect(disagree(presence, unavailable("GET /live-data", 503, "down."))).toBe(
      false,
    );
  });
});

describe("when tallying the states on a screen", () => {
  it("should count fresh and stale samples separately", () => {
    const fresh = sampled(1, "GET /a", iso(60_000));
    const stale = sampled(1, "GET /b", iso(60 * 60_000));
    const tally = tallyStates([fresh, stale, live(1, "GET /c", iso(0))], NOW);
    expect(tally).toEqual([
      { state: "live", stale: false, word: "Live", glyph: "\u{25CF}", count: 1 },
      { state: "sampled", stale: false, word: "Sampled", glyph: "\u{25D0}", count: 1 },
      { state: "sampled", stale: true, word: "Stale", glyph: "\u{25D0}", count: 1 },
    ]);
  });

  it("should omit states with no readings rather than showing a zero row", () => {
    expect(tallyStates([live(1, "GET /a", iso(0))], NOW)).toHaveLength(1);
  });
});

describe("when stamping a read time", () => {
  it("should give both the absolute and the relative time", () => {
    expect(formatReadStamp(iso(2 * 60_000), NOW)).toBe("14:01:22 UTC (2m ago)");
  });

  it("should return null for an unparseable timestamp rather than inventing one", () => {
    expect(formatReadStamp("not-a-date", NOW)).toBeNull();
  });
});
