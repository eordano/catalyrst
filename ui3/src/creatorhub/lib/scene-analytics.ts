// Pure data model + derivation math for creator scene analytics, ported from
// the upstream Creator Hub (packages/creator-hub, feat/scene-metrics).
// All numeric fields are nullable: null means masked / not enough data.

import { realmDeepLink, worldRealmUrl } from "../../data/deepLink";

export type SceneType = "genesis" | "world";

export type SceneMetricsWindow = {
  users: number | null;
  newUsers: number | null;
  returningUsers: number | null;
  visits: number | null;
  medianActiveTimeS: number | null;
  totalActiveHours: number | null;
  afkTimePct: number | null;
  peakConcurrentUsers: number | null;
  medianFps: number | null;
  perfSessions: number | null;
  dau: number | null;
  desktopUsers: number | null;
  mobileUsers: number | null;
  messagesSent: number | null;
  emotesPlayed: number | null;
  avgSessionActiveTimeS: number | null;
};

export type SceneRetention = {
  cohort: number | null;
  d1: number | null;
  d7: number | null;
  d30: number | null;
};

export type SceneRetentionPoint = {
  date: string;
  cohort: number;
  d1: number | null;
  d7: number | null;
  d30: number | null;
};

export type SceneDailyStats = {
  date: string;
  visits: number | null;
  uniqueUsers: number | null;
  newUsers: number | null;
  medianActiveTimeS: number | null;
  peakConcurrentUsers: number | null;
  messagesSent: number | null;
  emotesPlayed: number | null;
};

export type SceneStats = {
  sceneType: SceneType;
  sceneId: string;
  title: string | null;
  deployerAddress: string | null;
  windows: {
    yesterday: SceneMetricsWindow;
    last_7d: SceneMetricsWindow;
    last_30d: SceneMetricsWindow;
  };
  retention: SceneRetention;
  retentionSeries: SceneRetentionPoint[];
  daily: SceneDailyStats[];
  deployDates: string[];
};

export type CreatorScenesStats = {
  address: string;
  asOf: string;
  scenes: SceneStats[];
};

export type RangeDays = 7 | 30 | 90;

export type DailyPoint = { date: string; value: number | null };

export type ChartSeries = {
  key: string;
  label: string;
  color: string;
  points: DailyPoint[];
};

export type RetentionKey = "d1" | "d7" | "d30";

export type CsvRow = {
  date: string;
  visits: number;
  uniqueUsers: number;
  newUsers: number;
  medianActiveTimeS: number;
  peakConcurrentUsers: number;
  messagesSent: number;
  emotesPlayed: number;
};

export function sceneDisplayName(
  scene: Pick<SceneStats, "sceneType" | "title" | "sceneId">,
): string {
  if (scene.sceneType === "world") return scene.sceneId;
  return scene.title ?? scene.sceneId;
}

export function sceneKey(
  scene: Pick<SceneStats, "sceneType" | "sceneId">,
): string {
  return `${scene.sceneType}:${scene.sceneId}`;
}

type WindowKey = "last_7d" | "last_30d";

export const WINDOW_BY_RANGE: Partial<Record<RangeDays, WindowKey>> = {
  7: "last_7d",
  30: "last_30d",
};

export function addDays(date: string, days: number): string {
  const d = new Date(`${date}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}

function rangeDates(asOf: string, days: number): string[] {
  const dates: string[] = [];
  for (let i = days - 1; i >= 0; i--) {
    dates.push(addDays(asOf, -i));
  }
  return dates;
}

function dailyInRange(
  scene: SceneStats,
  start: string,
  end: string,
): SceneDailyStats[] {
  return scene.daily.filter((row) => row.date >= start && row.date <= end);
}

function weightedMedian(pairs: Array<[value: number, weight: number]>): number {
  const weighted = pairs
    .filter(([, weight]) => weight > 0)
    .sort((a, b) => a[0] - b[0]);
  const total = weighted.reduce((sum, [, weight]) => sum + weight, 0);
  if (total === 0) return 0;
  let cumulative = 0;
  for (const [value, weight] of weighted) {
    cumulative += weight;
    if (cumulative >= total / 2) return value;
  }
  return weighted[weighted.length - 1]![0];
}

export function buildCsvRows(
  scenes: SceneStats[],
  asOf: string,
  days: RangeDays,
): CsvRow[] {
  const byDate = new Map<string, SceneDailyStats[]>();
  for (const scene of scenes) {
    for (const row of scene.daily) {
      const rows = byDate.get(row.date);
      if (rows) rows.push(row);
      else byDate.set(row.date, [row]);
    }
  }
  return rangeDates(asOf, days).map((date) => {
    const rows = byDate.get(date) ?? [];
    return {
      date,
      visits: rows.reduce((sum, row) => sum + (row.visits ?? 0), 0),
      uniqueUsers: rows.reduce((sum, row) => sum + (row.uniqueUsers ?? 0), 0),
      newUsers: rows.reduce((sum, row) => sum + (row.newUsers ?? 0), 0),
      medianActiveTimeS: weightedMedian(
        rows.map((row): [number, number] => [
          row.medianActiveTimeS ?? 0,
          row.visits ?? 0,
        ]),
      ),
      peakConcurrentUsers: rows.reduce(
        (max, row) => Math.max(max, row.peakConcurrentUsers ?? 0),
        0,
      ),
      messagesSent: rows.reduce((sum, row) => sum + (row.messagesSent ?? 0), 0),
      emotesPlayed: rows.reduce((sum, row) => sum + (row.emotesPlayed ?? 0), 0),
    };
  });
}

export function toCsv(rows: CsvRow[]): string {
  const header =
    "date,visits,unique_users,new_users,median_active_time_s,peak_concurrent_users,messages_sent,emotes_played";
  const lines = rows.map((row) =>
    [
      row.date,
      row.visits,
      row.uniqueUsers,
      row.newUsers,
      row.medianActiveTimeS,
      row.peakConcurrentUsers,
      row.messagesSent,
      row.emotesPlayed,
    ].join(","),
  );
  return [header, ...lines].join("\n");
}

export function getLastDeploy(scene: SceneStats): string | null {
  return scene.deployDates.length
    ? scene.deployDates.reduce((a, b) => (a > b ? a : b))
    : null;
}

export function retentionPoints(
  scene: SceneStats,
  key: RetentionKey,
): DailyPoint[] {
  return [...scene.retentionSeries]
    .sort((a, b) => (a.date < b.date ? -1 : 1))
    .map((point) => ({ date: point.date, value: point[key] }));
}

export function isSeriesEmpty(points: DailyPoint[]): boolean {
  return points.every((point) => point.value === null);
}

export function tailPoints(points: DailyPoint[], days: RangeDays): DailyPoint[] {
  return days >= points.length ? points : points.slice(points.length - days);
}

export function buildSocialSeries(
  scenes: SceneStats[],
  asOf: string,
  days: RangeDays,
): { messages: DailyPoint[]; emotes: DailyPoint[] } {
  const byDate = new Map<string, SceneDailyStats[]>();
  for (const scene of scenes) {
    for (const row of scene.daily) {
      const rows = byDate.get(row.date);
      if (rows) rows.push(row);
      else byDate.set(row.date, [row]);
    }
  }
  const dates = rangeDates(asOf, days);
  return {
    messages: dates.map((date) => ({
      date,
      value: (byDate.get(date) ?? []).reduce(
        (sum, row) => sum + (row.messagesSent ?? 0),
        0,
      ),
    })),
    emotes: dates.map((date) => ({
      date,
      value: (byDate.get(date) ?? []).reduce(
        (sum, row) => sum + (row.emotesPlayed ?? 0),
        0,
      ),
    })),
  };
}

// Percent change of the daily mean of a picked field between the current
// window and the immediately preceding window of equal length, rounded to one
// decimal. Null when either window has no data or the previous mean is zero.
export function dailyMeanDelta(
  scenes: SceneStats[],
  asOf: string,
  days: RangeDays,
  pick: (row: SceneDailyStats) => number | null,
): number | null {
  const start = addDays(asOf, -(days - 1));
  const prevEnd = addDays(start, -1);
  const prevStart = addDays(prevEnd, -(days - 1));

  const windowMean = (from: string, to: string): number | null => {
    const values = scenes
      .flatMap((scene) => dailyInRange(scene, from, to).map(pick))
      .filter((value): value is number => value !== null);
    if (values.length === 0) return null;
    return values.reduce((sum, value) => sum + value, 0) / values.length;
  };

  const current = windowMean(start, asOf);
  const previous = windowMean(prevStart, prevEnd);
  if (current === null || previous === null || previous === 0) return null;
  return Math.round(((current - previous) / previous) * 1000) / 10;
}

export type PortfolioSort = "recent" | "visitors" | "name";

export const PORTFOLIO_SORTS: PortfolioSort[] = ["recent", "visitors", "name"];

export const PORTFOLIO_WINDOW = "last_30d" as const;

export const D7_POSITIVE_THRESHOLD = 20;

export function comparePortfolio(
  a: SceneStats,
  b: SceneStats,
  sort: PortfolioSort,
): number {
  if (sort === "name") {
    return sceneDisplayName(a).localeCompare(sceneDisplayName(b), undefined, {
      sensitivity: "base",
    });
  }
  if (sort === "visitors") {
    return (
      (b.windows[PORTFOLIO_WINDOW].users ?? -1) -
      (a.windows[PORTFOLIO_WINDOW].users ?? -1)
    );
  }
  return (getLastDeploy(b) ?? "").localeCompare(getLastDeploy(a) ?? "");
}

export const NO_DATA = "\u{2014}";

export function formatNumber(value: number | null): string {
  return value === null ? NO_DATA : value.toLocaleString();
}

export function formatPercent(value: number | null): string {
  return value === null ? NO_DATA : `${Math.round(value)}%`;
}

export function formatMinutes(seconds: number | null): string {
  return seconds === null
    ? NO_DATA
    : `${Math.round((seconds / 60) * 10) / 10} min`;
}

// Accepts a plain date ("2026-07-12", rendered as a UTC calendar date) or a
// full ISO timestamp (rendered as a date in the viewer's timezone).
export function formatDate(date: string): string {
  const isTimestamp = date.includes("T");
  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    ...(isTimestamp ? {} : { timeZone: "UTC" }),
  }).format(new Date(isTimestamp ? date : `${date}T00:00:00Z`));
}

export function formatDateTime(value: string): string {
  if (!value.includes("T")) return formatDate(value);
  const date = new Date(value);
  const day = new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(date);
  const time = new Intl.DateTimeFormat(undefined, {
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
  return `${day} \u{2022} ${time}`;
}

export { truncateAddress } from "../../data/format";

export const SUBMIT_EVENT_URL = "https://decentraland.org/events/submit";

/**
 * Pass `realmBaseUrl` for anything self-hosted: without it the only link this
 * can offer is decentraland.org's, which forwards `realm` solely for realms it
 * whitelists and otherwise drops it, landing the visitor in Genesis.
 */
export function jumpInUrl(scene: SceneStats, realmBaseUrl?: string): string {
  const position =
    scene.sceneType === "world" ? "0,0" : scene.sceneId.replace("|", ",");
  if (realmBaseUrl) {
    const realm =
      scene.sceneType === "world"
        ? worldRealmUrl(realmBaseUrl, scene.sceneId)
        : realmBaseUrl;
    return realmDeepLink(realm, position);
  }
  if (scene.sceneType === "world") {
    return `https://decentraland.org/play/?realm=${encodeURIComponent(scene.sceneId)}`;
  }
  return `https://decentraland.org/play/?position=${encodeURIComponent(position)}`;
}

export function csvFileName(scene: SceneStats, asOf: string): string {
  const scope = scene.sceneId.replace(/[^a-z0-9.-]/gi, "_");
  return `scene-metrics-${scope}-${asOf}.csv`;
}
