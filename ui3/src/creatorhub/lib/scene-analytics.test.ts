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
  it("should use the world name for worlds and the title, then the coords, for genesis scenes", () => {
    expect(sceneDisplayName(healthyWorldScene)).toBe("kickoff.dcl.eth");
    expect(sceneDisplayName(healthyGenesisScene)).toBe("Plaza Corner");
    expect(sceneDisplayName({ ...healthyGenesisScene, title: null })).toBe(
      "-3|-2",
    );
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
    const rows = buildCsvRows([zeroTrafficScene], FIXTURE_AS_OF, 7);
    expect(rows).toHaveLength(7);
    for (const row of rows) {
      expect(row.visits).toBeNull();
      expect(row.uniqueUsers).toBeNull();
    }
  });
});

describe("when building retention series points", () => {
  it("should give 90 oldest-first points, tail to the range ending at asOf, and read masked or missing series as empty", () => {
    const points = retentionPoints(healthyGenesisScene, "d7");
    expect(points).toHaveLength(90);
    expect(points[0]!.date < points[points.length - 1]!.date).toBe(true);
    expect(points.every((point) => point.value !== null)).toBe(true);
    const tail = tailPoints(points, 7);
    expect(tail).toHaveLength(7);
    expect(tail[tail.length - 1]!.date).toBe(FIXTURE_AS_OF);
    expect(isSeriesEmpty(retentionPoints(maskedRetentionScene, "d1"))).toBe(true);
    expect(isSeriesEmpty(retentionPoints(zeroTrafficScene, "d30"))).toBe(true);
  });
});

describe("when building the social series", () => {
  it("should sum messages and emotes across scenes per day and preserve missing daily data", () => {
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
    const empty = buildSocialSeries([zeroTrafficScene], FIXTURE_AS_OF, 7);
    expect(empty.messages.every((point) => point.value === null)).toBe(true);
  });
});

describe("when computing the daily mean delta", () => {
  it("should return null without a prior window and 0 for a flat series", () => {
    expect(
      dailyMeanDelta(
        [healthyGenesisScene],
        FIXTURE_AS_OF,
        90,
        (row) => row.medianActiveTimeS,
      ),
    ).toBeNull();
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
  it("should sort by name case-insensitively, by 30-day visitors descending, or by most recent deploy", () => {
    const sortBy = (key: "name" | "visitors" | "recent") =>
      [...creatorScenesStatsFixture.scenes].sort((a, b) => comparePortfolio(a, b, key));
    expect(sortBy("name").map(sceneDisplayName)).toEqual([
      "hidden-gem.dcl.eth",
      "kickoff.dcl.eth",
      "Plaza Corner",
      "Quiet Parcel",
    ]);
    expect(sortBy("visitors").slice(0, 2)).toEqual([healthyWorldScene, healthyGenesisScene]);
    expect(sortBy("recent").slice(0, 2)).toEqual([zeroTrafficScene, healthyWorldScene]);
    expect(getLastDeploy(healthyGenesisScene)).toBe("2026-07-10");
    expect(getLastDeploy(maskedRetentionScene)).toBeNull();
  });
});

describe("when formatting values and dates", () => {
  it("should render numbers, percents, minutes with an em dash for null, and add days across month boundaries in UTC", () => {
    expect(formatNumber(1500)).toBe((1500).toLocaleString());
    expect(formatNumber(null)).toBe("\u{2014}");
    expect(formatPercent(36.2)).toBe("36%");
    expect(formatPercent(null)).toBe("\u{2014}");
    expect(formatMinutes(149)).toBe("2.5 min");
    expect(formatMinutes(null)).toBe("\u{2014}");
    expect(addDays("2026-07-01", -1)).toBe("2026-06-30");
    expect(addDays("2026-07-21", -89)).toBe("2026-04-23");
  });
});

describe("when building jump-in URLs", () => {
  it("should use the realm for worlds, the position for genesis scenes, and a protocol deep link once a realm base is known", () => {
    expect(jumpInUrl(healthyWorldScene)).toBe(
      "https://decentraland.org/play/?realm=kickoff.dcl.eth",
    );
    expect(jumpInUrl(healthyGenesisScene)).toBe(
      "https://decentraland.org/play/?position=-3%2C-2",
    );
    expect(jumpInUrl(healthyWorldScene, "https://realm.example.org/worlds")).toBe(
      "decentraland://realm=https%3A%2F%2Frealm.example.org%2Fworlds%2Fworld%2Fkickoff.dcl.eth&position=0%2C0",
    );
    expect(jumpInUrl(healthyGenesisScene, "https://realm.example.org")).toBe(
      "decentraland://realm=https%3A%2F%2Frealm.example.org&position=-3%2C-2",
    );
  });
});
