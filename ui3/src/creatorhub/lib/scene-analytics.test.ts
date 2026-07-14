import { describe, expect, it } from "vitest";
import {
  addDays,
  buildCsvRows,
  buildSocialSeries,
  comparePortfolio,
  dailyMeanDelta,
  formatMinutes,
  formatNumber,
  formatPercent,
  getLastDeploy,
  isSeriesEmpty,
  jumpInUrl,
  retentionPoints,
  sceneDisplayName,
  tailPoints,
  toCsv,
} from "./scene-analytics";
import {
  FIXTURE_AS_OF,
  creatorScenesStatsFixture,
  healthyGenesisScene,
  healthyWorldScene,
  maskedRetentionScene,
  zeroTrafficScene,
} from "./scene-analytics.fixtures";

describe("when displaying scene names", () => {
  it("should use the world name for worlds and the title for genesis scenes", () => {
    expect(sceneDisplayName(healthyWorldScene)).toBe("kickoff.dcl.eth");
    expect(sceneDisplayName(healthyGenesisScene)).toBe("Plaza Corner");
    expect(sceneDisplayName({ ...healthyGenesisScene, title: null })).toBe(
      "-3|-2",
    );
  });
});

describe("when adding days to a date", () => {
  it("should move across month boundaries in UTC", () => {
    expect(addDays("2026-07-01", -1)).toBe("2026-06-30");
    expect(addDays("2026-07-21", -89)).toBe("2026-04-23");
  });
});

describe("when exporting a scene to CSV", () => {
  it("should produce a header plus one row per day with aggregated values", () => {
    const csv = toCsv(buildCsvRows([healthyWorldScene], FIXTURE_AS_OF, 7));
    const lines = csv.split("\n");
    expect(lines).toHaveLength(8);
    expect(lines[0]).toBe(
      "date,visits,unique_users,new_users,median_active_time_s,peak_concurrent_users,messages_sent,emotes_played",
    );
    expect(lines[lines.length - 1]).toBe("2026-07-21,40,25,4,180,8,80,42");
  });

  it("should zero-fill days with no data", () => {
    const rows = buildCsvRows([zeroTrafficScene], FIXTURE_AS_OF, 7);
    expect(rows).toHaveLength(7);
    for (const row of rows) {
      expect(row.visits).toBe(0);
      expect(row.uniqueUsers).toBe(0);
    }
  });
});

describe("when reading the last deploy date", () => {
  it("should return the most recent deploy or null", () => {
    expect(getLastDeploy(healthyGenesisScene)).toBe("2026-07-10");
    expect(getLastDeploy(maskedRetentionScene)).toBeNull();
  });
});

describe("when building retention series points", () => {
  it("should return 90 points oldest-first with non-null values for a healthy scene", () => {
    const points = retentionPoints(healthyGenesisScene, "d7");
    expect(points).toHaveLength(90);
    expect(points[0]!.date < points[points.length - 1]!.date).toBe(true);
    expect(points.every((point) => point.value !== null)).toBe(true);
  });

  it("should report masked series as empty", () => {
    const masked = retentionPoints(maskedRetentionScene, "d1");
    expect(isSeriesEmpty(masked)).toBe(true);
  });

  it("should treat a scene with no series as empty", () => {
    expect(isSeriesEmpty(retentionPoints(zeroTrafficScene, "d30"))).toBe(true);
  });

  it("should tail to the requested range ending at asOf", () => {
    const points = tailPoints(retentionPoints(healthyGenesisScene, "d7"), 7);
    expect(points).toHaveLength(7);
    expect(points[points.length - 1]!.date).toBe(FIXTURE_AS_OF);
  });
});

describe("when building the social series", () => {
  it("should sum messages and emotes across scenes per day", () => {
    const social = buildSocialSeries(
      creatorScenesStatsFixture.scenes,
      FIXTURE_AS_OF,
      7,
    );
    expect(social.messages).toHaveLength(7);
    for (const point of social.messages) {
      expect(point.value).toBe(110 + 2 + 80);
    }
    for (const point of social.emotes) {
      expect(point.value).toBe(55 + 1 + 42);
    }
  });

  it("should zero-fill for scenes without daily data", () => {
    const social = buildSocialSeries([zeroTrafficScene], FIXTURE_AS_OF, 7);
    expect(social.messages.every((point) => point.value === 0)).toBe(true);
  });
});

describe("when computing the daily mean delta", () => {
  it("should return null when there is no prior window", () => {
    expect(
      dailyMeanDelta(
        [healthyGenesisScene],
        FIXTURE_AS_OF,
        90,
        (row) => row.medianActiveTimeS,
      ),
    ).toBeNull();
  });

  it("should return 0 for a flat series", () => {
    expect(
      dailyMeanDelta(
        [healthyWorldScene],
        FIXTURE_AS_OF,
        7,
        (row) => row.medianActiveTimeS,
      ),
    ).toBe(0);
  });
});

describe("when sorting the portfolio", () => {
  it("should sort by name case-insensitively", () => {
    const sorted = [...creatorScenesStatsFixture.scenes].sort((a, b) =>
      comparePortfolio(a, b, "name"),
    );
    expect(sorted.map(sceneDisplayName)).toEqual([
      "hidden-gem.dcl.eth",
      "kickoff.dcl.eth",
      "Plaza Corner",
      "Quiet Parcel",
    ]);
  });

  it("should sort by 30-day unique visitors descending", () => {
    const sorted = [...creatorScenesStatsFixture.scenes].sort((a, b) =>
      comparePortfolio(a, b, "visitors"),
    );
    expect(sorted[0]).toBe(healthyWorldScene);
    expect(sorted[1]).toBe(healthyGenesisScene);
  });

  it("should sort by most recent deploy first", () => {
    const sorted = [...creatorScenesStatsFixture.scenes].sort((a, b) =>
      comparePortfolio(a, b, "recent"),
    );
    expect(sorted[0]).toBe(zeroTrafficScene);
    expect(sorted[1]).toBe(healthyWorldScene);
  });
});

describe("when formatting values", () => {
  it("should render numbers, percents, and minutes with em dash for null", () => {
    expect(formatNumber(1500)).toBe((1500).toLocaleString());
    expect(formatNumber(null)).toBe("—");
    expect(formatPercent(36.2)).toBe("36%");
    expect(formatPercent(null)).toBe("—");
    expect(formatMinutes(149)).toBe("2.5 min");
    expect(formatMinutes(null)).toBe("—");
  });
});

describe("when building jump-in URLs", () => {
  it("should use the realm for worlds and position for genesis scenes", () => {
    expect(jumpInUrl(healthyWorldScene)).toBe(
      "https://decentraland.org/play/?realm=kickoff.dcl.eth",
    );
    expect(jumpInUrl(healthyGenesisScene)).toBe(
      "https://decentraland.org/play/?position=-3%2C-2",
    );
  });
});
