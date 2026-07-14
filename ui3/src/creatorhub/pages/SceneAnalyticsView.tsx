import { useCallback, useMemo, useState } from "react";
import type * as React from "react";
import EmptyState from "../../components/EmptyState";
import Spinner from "../../atoms/Spinner";
import AnalyticsChart from "../components/AnalyticsChart";
import {
  D7_POSITIVE_THRESHOLD,
  NO_DATA,
  PORTFOLIO_SORTS,
  PORTFOLIO_WINDOW,
  SUBMIT_EVENT_URL,
  WINDOW_BY_RANGE,
  buildCsvRows,
  buildSocialSeries,
  comparePortfolio,
  csvFileName,
  dailyMeanDelta,
  formatDateTime,
  formatMinutes,
  formatNumber,
  formatPercent,
  getLastDeploy,
  isSeriesEmpty,
  jumpInUrl,
  retentionPoints,
  sceneDisplayName,
  sceneKey,
  tailPoints,
  toCsv,
  truncateAddress,
} from "../lib/scene-analytics";
import type {
  ChartSeries,
  PortfolioSort,
  RangeDays,
  RetentionKey,
  SceneStats,
  SceneType,
} from "../lib/scene-analytics";
import "./sceneanalyticsview.css";

const RANGE_OPTIONS: RangeDays[] = [7, 30, 90];
const OVERVIEW_WINDOW = "last_30d" as const;

const RANGE_LABELS: Record<RangeDays, string> = {
  7: "Last 7 days",
  30: "Last 30 days",
  90: "Last 90 days",
};

const COLOR = {
  ruby: "var(--brand)",
  messages: "var(--info)",
  emotes: "var(--success)",
} as const;

const TIP = {
  totalVisits:
    "A visit is a sessionized presence in your scene; a new visit starts after a 10-minute gap.",
  uniqueVisits: "Distinct accounts that visited your scene in the window.",
  newUsers: "Accounts whose first-ever visit to your scene falls in the window.",
  concurrentUsers:
    "Peak simultaneous accounts in your scene, measured in 5-minute buckets.",
  dau: "Average distinct daily accounts over the window.",
  d7: "Share of new visitors who came back at least once within the 7 days after their first visit.",
  avgPlaytime: "Median active (non-AFK) time per visit.",
  afkTime: "Share of dwell time spent idle (2\u{2013}10 minute gaps).",
  revenue: "In-scene monetization isn't available yet.",
  device:
    "Distinct accounts by device. Cross-platform players count in both, so the split isn't additive.",
  retentionD1:
    "Share of each day's new visitors who returned the next day.",
  retentionD7:
    "Share of each day's new visitors who returned within 7 days.",
  retentionD30:
    "Share of each day's new visitors who returned within 30 days.",
  avgSession: "Mean active (non-AFK) seconds per account session.",
} as const;

const NOT_ENOUGH_DATA = "Not enough data";

function glyph(path: React.ReactNode, size = 16): React.ReactElement {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {path}
    </svg>
  );
}

const ChevronLeftGlyph = glyph(<path d="m14 6-6 6 6 6" />, 22);
const SearchGlyph = glyph(
  <>
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.5-3.5" />
  </>,
);
const DownloadGlyph = glyph(
  <>
    <path d="M12 4v11" />
    <path d="m7 11 5 5 5-5" />
    <path d="M4 20h16" />
  </>,
);
const InfoGlyph = glyph(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 11v5" />
    <path d="M12 8h.01" />
  </>,
  14,
);
const GlobeGlyph = glyph(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18" />
    <path d="M12 3a15 15 0 0 1 0 18a15 15 0 0 1 0-18" />
  </>,
  14,
);
const LockGlyph = glyph(
  <>
    <rect x="5" y="11" width="14" height="9" rx="2" />
    <path d="M8 11V8a4 4 0 0 1 8 0v3" />
  </>,
  14,
);
const PinGlyph = glyph(
  <>
    <path d="M12 21s-7-5.5-7-11a7 7 0 0 1 14 0c0 5.5-7 11-7 11Z" />
    <circle cx="12" cy="10" r="2.5" />
  </>,
  14,
);
const CalendarGlyph = glyph(
  <>
    <rect x="4" y="5" width="16" height="16" rx="2" />
    <path d="M8 3v4M16 3v4M4 11h16" />
  </>,
);
const EditGlyph = glyph(
  <path d="M4 20h4L19.5 8.5a2.1 2.1 0 0 0-3-3L5 17Z" />,
);
const LinkGlyph = glyph(
  <>
    <path d="M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1.5 1.5" />
    <path d="M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7L12.5 19" />
  </>,
);
const ArrowRightGlyph = glyph(
  <>
    <path d="M4 12h16" />
    <path d="m14 6 6 6-6 6" />
  </>,
);
const ChartEmptyGlyph = glyph(
  <>
    <path d="M4 20V10" />
    <path d="M10 20V4" />
    <path d="M16 20v-6" />
    <path d="M2 20h20" />
  </>,
  34,
);

export type SceneAnalyticsPhase = "signed-out" | "loading" | "error" | "ready";

export type SceneSelection = { sceneType: SceneType; sceneId: string };

export type SceneAnalyticsViewProps = {
  phase: SceneAnalyticsPhase;
  error?: string | null;
  scenes?: SceneStats[];
  asOf?: string | null;
  selected?: SceneSelection | null;
  rankingEnabled?: boolean;
  creatorHref?: string;
  operatorHref?: string;
  thumbnails?: Record<string, string>;
  /**
   * Public base URL of the realm serving these scenes. Given one, jump-in
   * becomes a `decentraland://` deep link that reaches this node; without it
   * the only link available is decentraland.org's, which drops a custom realm.
   */
  realmBaseUrl?: string;
  worldAccess?: Record<string, "public" | "private">;
  editHrefFor?: (scene: SceneStats) => string | null;
  onConnect?: () => void;
  onRetry?: () => void;
  onOpenScene?: (scene: SceneStats) => void;
  onBack?: () => void;
  onExportCsv?: (scene: SceneStats) => void;
};

function downloadSceneCsv(scene: SceneStats, asOf: string) {
  if (typeof document === "undefined") return;
  const csv = toCsv(buildCsvRows([scene], asOf, 90));
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = csvFileName(scene, asOf);
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

function rowValue(formatted: string): string {
  return formatted === NO_DATA ? "-" : formatted;
}

function Tip({ text }: { text: string }) {
  return (
    <span className="sa-tip" title={text} aria-label={text} role="img">
      {InfoGlyph}
    </span>
  );
}

function MetricTile({
  label,
  tip,
  valueClassName,
  extra,
  children,
}: {
  label: string;
  tip?: string;
  valueClassName?: string;
  extra?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="sa-tile">
      <div className="sa-tilehead">
        <span className="sa-tilelabel">{label}</span>
        {tip && <Tip text={tip} />}
      </div>
      <div className={`sa-tilevalue${valueClassName ? ` ${valueClassName}` : ""}`}>
        {children}
      </div>
      {extra}
    </div>
  );
}

function MinutesValue({ seconds }: { seconds: number | null }) {
  if (seconds === null) return <>{NO_DATA}</>;
  return (
    <>
      {Math.round((seconds / 60) * 10) / 10}
      <span className="sa-tileunit">min</span>
    </>
  );
}

function DeltaChip({ pct }: { pct: number | null }) {
  if (pct === null || pct === 0) return null;
  const up = pct > 0;
  return (
    <span className={`sa-delta ${up ? "is-up" : "is-down"}`}>
      {`${up ? "\u{2191}" : "\u{2193}"}${Math.abs(pct)}%`}
    </span>
  );
}

function DateRangeControl({
  range,
  onChange,
}: {
  range: RangeDays;
  onChange: (range: RangeDays) => void;
}) {
  return (
    <label className="sa-rangecontrol">
      <span>Date Range</span>
      <select
        value={String(range)}
        onChange={(event) => onChange(Number(event.target.value) as RangeDays)}
      >
        {RANGE_OPTIONS.map((option) => (
          <option key={option} value={String(option)}>
            {RANGE_LABELS[option]}
          </option>
        ))}
      </select>
    </label>
  );
}

function PortfolioRow({
  scene,
  thumbnail,
  asOf,
  onOpen,
}: {
  scene: SceneStats;
  thumbnail?: string;
  asOf: string;
  onOpen: () => void;
}) {
  const w = scene.windows[PORTFOLIO_WINDOW];
  const d7 = scene.retention.d7;

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        onOpen();
      }
    },
    [onOpen],
  );

  const d7Class =
    d7 === null
      ? ""
      : d7 >= D7_POSITIVE_THRESHOLD
        ? " is-positive"
        : " is-negative";

  return (
    <div
      className="sa-row"
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={handleKeyDown}
    >
      <div className="sa-colplace">
        <span className="sa-thumb">
          {thumbnail && <img src={thumbnail} alt={sceneDisplayName(scene)} />}
        </span>
        <span className="sa-rowname">{sceneDisplayName(scene)}</span>
      </div>
      <div className="sa-cols">
        <span className="sa-col">{rowValue(formatNumber(w.newUsers))}</span>
        <span className="sa-col">{rowValue(formatNumber(w.dau))}</span>
        <span className={`sa-col${d7Class}`}>{rowValue(formatPercent(d7))}</span>
        <span className="sa-col">
          {rowValue(formatMinutes(w.medianActiveTimeS))}
        </span>
      </div>
      <div
        className="sa-rowactions"
        onClick={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          className="sa-iconbutton"
          title="Export Analytics"
          aria-label="Export Analytics"
          onClick={() => downloadSceneCsv(scene, asOf)}
        >
          {DownloadGlyph}
        </button>
      </div>
    </div>
  );
}

function ScenePortfolio({
  scenes,
  thumbnails,
  asOf,
  onOpen,
}: {
  scenes: SceneStats[];
  thumbnails: Record<string, string>;
  asOf: string;
  onOpen: (scene: SceneStats) => void;
}) {
  const [sort, setSort] = useState<PortfolioSort>("recent");
  const [query, setQuery] = useState("");
  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = needle
      ? scenes.filter((scene) =>
          sceneDisplayName(scene).toLowerCase().includes(needle),
        )
      : scenes;
    return [...filtered].sort((a, b) => comparePortfolio(a, b, sort));
  }, [scenes, sort, query]);

  const sortLabels: Record<PortfolioSort, string> = {
    recent: "Recently updated",
    visitors: "Unique visitors",
    name: "Name A-Z",
  };

  return (
    <>
      <h1 className="sa-title">Analytics</h1>
      <div className="sa-filter">
        <span className="sa-count">
          {scenes.length} {scenes.length === 1 ? "Place" : "Places"}
        </span>
        <div className="sa-filterright">
          <label className="sa-sort">
            <span>Sort by</span>
            <select
              value={sort}
              onChange={(event) => setSort(event.target.value as PortfolioSort)}
            >
              {PORTFOLIO_SORTS.map((option) => (
                <option key={option} value={option}>
                  {sortLabels[option]}
                </option>
              ))}
            </select>
          </label>
          <label className="sa-search">
            {SearchGlyph}
            <input
              type="text"
              value={query}
              placeholder="Search"
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>
        </div>
      </div>
      <div className="sa-table">
        <div className="sa-tablehead">
          <span className="sa-colplace">Place</span>
          <div className="sa-cols">
            <span className="sa-col">New Users</span>
            <span className="sa-col">Daily Active Users</span>
            <span className="sa-col">D7 Retention</span>
            <span className="sa-col">Avg. Playtime</span>
          </div>
          <span className="sa-rowactions" aria-hidden="true" />
        </div>
        {visible.map((scene) => (
          <PortfolioRow
            key={sceneKey(scene)}
            scene={scene}
            thumbnail={thumbnails[sceneKey(scene)]}
            asOf={asOf}
            onOpen={() => onOpen(scene)}
          />
        ))}
      </div>
    </>
  );
}

function SceneCard({
  scene,
  thumbnail,
  access,
  editHref,
  realmBaseUrl,
}: {
  scene: SceneStats;
  thumbnail?: string;
  access?: "public" | "private";
  editHref?: string | null;
  realmBaseUrl?: string;
}) {
  const lastDeploy = getLastDeploy(scene);
  const isWorld = scene.sceneType === "world";
  const accessKnown = !isWorld || access !== undefined;
  const isPublic = !isWorld || access === "public";
  const [copied, setCopied] = useState(false);

  const handleCopyUrl = useCallback(() => {
    void navigator.clipboard?.writeText(jumpInUrl(scene, realmBaseUrl)).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [scene, realmBaseUrl]);

  return (
    <div className="sa-scenecard">
      <div className="sa-scenethumb">
        {thumbnail ? (
          <img src={thumbnail} alt={sceneDisplayName(scene)} />
        ) : (
          <div className="sa-scenethumbempty" />
        )}
      </div>
      <div className="sa-scenebody">
        <h2 className="sa-scenename">{sceneDisplayName(scene)}</h2>

        <div className="sa-scenemeta">
          <span className="sa-scenemetalabel">Access</span>
          <span className="sa-scenemetavalue">
            {accessKnown ? (
              <>
                {isPublic ? GlobeGlyph : LockGlyph}
                <span>{isPublic ? "Public" : "Private"}</span>
              </>
            ) : (
              <span>{isWorld ? "World" : "Genesis"}</span>
            )}
          </span>
        </div>
        <div className="sa-scenemeta">
          <span className="sa-scenemetalabel">Published in</span>
          <span className="sa-scenemetavalue">
            {PinGlyph}
            <span>{scene.sceneId}</span>
          </span>
        </div>
        <div className="sa-scenemeta">
          <span className="sa-scenemetalabel">Last published by</span>
          <span className="sa-scenemetavalue">
            {scene.deployerAddress ? (
              <span title={scene.deployerAddress}>
                {truncateAddress(scene.deployerAddress)}
              </span>
            ) : (
              <span>{NO_DATA}</span>
            )}
          </span>
        </div>
        <div className="sa-scenemeta">
          <span className="sa-scenemetalabel">Last Update</span>
          <span className="sa-scenemetavalue">
            <span>{lastDeploy ? formatDateTime(lastDeploy) : NO_DATA}</span>
          </span>
        </div>
      </div>

      <div className="sa-scenefooter">
        <div className="sa-sceneactions">
          <a
            className="sa-sceneaction"
            href={SUBMIT_EVENT_URL}
            target="_blank"
            rel="noreferrer"
          >
            {CalendarGlyph}
            <span>Create Event</span>
          </a>
          {editHref ? (
            <a className="sa-sceneaction" href={editHref}>
              {EditGlyph}
              <span>Edit Scene</span>
            </a>
          ) : (
            <button
              type="button"
              className="sa-sceneaction"
              disabled
              title="This scene's project isn't available here"
            >
              {EditGlyph}
              <span>Edit Scene</span>
            </button>
          )}
          <button
            type="button"
            className="sa-sceneaction"
            onClick={handleCopyUrl}
          >
            {LinkGlyph}
            <span>{copied ? "Copied!" : "Copy URL"}</span>
          </button>
        </div>

        <a
          className="sa-jumpin"
          href={jumpInUrl(scene, realmBaseUrl)}
          target="_blank"
          rel="noreferrer"
        >
          Jump In
          <span className="sa-jumpinarrow">{ArrowRightGlyph}</span>
        </a>
      </div>
    </div>
  );
}

function OverviewSection({
  scene,
  asOf,
  rankingEnabled,
}: {
  scene: SceneStats;
  asOf: string;
  rankingEnabled: boolean;
}) {
  const w = scene.windows[OVERVIEW_WINDOW];
  const d7 = scene.retention.d7;

  return (
    <section className="sa-section">
      <h3 className="sa-sectiontitle">
        Overview
        <span className="sa-sectionsub">
          ({OVERVIEW_WINDOW.replace("last_", "")} updated {asOf})
        </span>
      </h3>

      {rankingEnabled && (
        <div className="sa-ranking">
          <span className="sa-rankinglabel">Places Ranking</span>
          <span className="sa-rankingvalue">{NO_DATA}</span>
          <span className="sa-rankinglink">View Leaderboard</span>
        </div>
      )}

      <div className="sa-overviewrow">
        <div className="sa-metricgroup">
          <MetricTile label="Total Visits" tip={TIP.totalVisits}>
            {formatNumber(w.visits)}
          </MetricTile>
          <div className="sa-metricgrouprow">
            <MetricTile label="Unique Visits" tip={TIP.uniqueVisits}>
              {formatNumber(w.users)}
            </MetricTile>
            <MetricTile label="New Users" tip={TIP.newUsers}>
              {formatNumber(w.newUsers)}
            </MetricTile>
          </div>
          <div className="sa-metricgrouprow">
            <MetricTile label="Concurrent Users" tip={TIP.concurrentUsers}>
              {formatNumber(w.peakConcurrentUsers)}
            </MetricTile>
            <MetricTile label="Daily Active Users" tip={TIP.dau}>
              {formatNumber(w.dau)}
            </MetricTile>
          </div>
        </div>
        <div className="sa-metricgroup">
          <MetricTile
            label="D7 Retention"
            tip={TIP.d7}
            valueClassName={d7 !== null ? "is-positive" : "is-muted"}
          >
            {d7 !== null ? `${Math.round(d7)}%` : NOT_ENOUGH_DATA}
          </MetricTile>
          <MetricTile label="Avg. Playtime" tip={TIP.avgPlaytime}>
            <MinutesValue seconds={w.medianActiveTimeS} />
          </MetricTile>
          <MetricTile label="AFK Time" tip={TIP.afkTime}>
            {formatPercent(w.afkTimePct)}
          </MetricTile>
          <MetricTile label="Revenue" tip={TIP.revenue} valueClassName="is-muted">
            {NO_DATA}
          </MetricTile>
          <div className="sa-metricgrouprow">
            <MetricTile label="Desktop" tip={TIP.device}>
              {formatNumber(w.desktopUsers)}
            </MetricTile>
            <MetricTile label="Mobile" tip={TIP.device}>
              {formatNumber(w.mobileUsers)}
            </MetricTile>
          </div>
        </div>
      </div>
    </section>
  );
}

function RetentionChart({
  scene,
  metric,
  label,
  tip,
  range,
  className,
}: {
  scene: SceneStats;
  metric: RetentionKey;
  label: string;
  tip?: string;
  range: RangeDays;
  className?: string;
}) {
  const points = useMemo(
    () => tailPoints(retentionPoints(scene, metric), range),
    [scene, metric, range],
  );
  const series: ChartSeries[] = [
    { key: metric, label, color: "url(#sa-retention-gradient)", points },
  ];

  return (
    <div className={`sa-retentionchart${className ? ` ${className}` : ""}`}>
      <div className="sa-tilehead">
        <span className="sa-charttitle">{label}</span>
        {tip && <Tip text={tip} />}
      </div>
      {isSeriesEmpty(points) ? (
        <div className="sa-chartempty">
          <span>{NOT_ENOUGH_DATA}</span>
        </div>
      ) : (
        <AnalyticsChart
          series={series}
          ariaLabel={label}
          unit="%"
          yMax={80}
          yStep={20}
          showDelta
        />
      )}
    </div>
  );
}

function RetentionSection({ scene }: { scene: SceneStats }) {
  const [range, setRange] = useState<RangeDays>(90);

  return (
    <section className="sa-section">
      <svg
        aria-hidden="true"
        focusable="false"
        width="0"
        height="0"
        style={{ position: "absolute" }}
      >
        <defs>
          {/* userSpaceOnUse spans the chart viewBox (0..720) so the gradient
              still renders for flat series, whose zero-height bounding box
              would make an objectBoundingBox gradient paint nothing. */}
          <linearGradient
            id="sa-retention-gradient"
            gradientUnits="userSpaceOnUse"
            x1="0"
            y1="0"
            x2="720"
            y2="0"
          >
            <stop offset="0%" stopColor="#C640CD" />
            <stop offset="100%" stopColor="#FF2D55" />
          </linearGradient>
        </defs>
      </svg>
      <div className="sa-sectionhead">
        <h3 className="sa-sectiontitle">Retention</h3>
        <DateRangeControl range={range} onChange={setRange} />
      </div>
      <div className="sa-retentionrow">
        <RetentionChart
          scene={scene}
          metric="d1"
          label="Day 1 Retention"
          tip={TIP.retentionD1}
          range={range}
        />
        <RetentionChart
          scene={scene}
          metric="d7"
          label="Day 7 Retention"
          tip={TIP.retentionD7}
          range={range}
        />
      </div>
      <RetentionChart
        scene={scene}
        metric="d30"
        label="Day 30 Retention"
        tip={TIP.retentionD30}
        range={range}
        className="is-fullwidth"
      />
    </section>
  );
}

function EngagementSection({
  scene,
  asOf,
}: {
  scene: SceneStats;
  asOf: string;
}) {
  const [range, setRange] = useState<RangeDays>(7);
  const windowKey = WINDOW_BY_RANGE[range];
  const w = windowKey ? scene.windows[windowKey] : null;
  const playtimeDelta = useMemo(
    () => dailyMeanDelta([scene], asOf, range, (row) => row.medianActiveTimeS),
    [scene, asOf, range],
  );
  const social = useMemo(
    () => buildSocialSeries([scene], asOf, range),
    [scene, asOf, range],
  );
  const socialSeries: ChartSeries[] = [
    {
      key: "messages",
      label: "Messages Sent",
      color: COLOR.messages,
      points: social.messages,
    },
    {
      key: "emotes",
      label: "Emotes Played",
      color: COLOR.emotes,
      points: social.emotes,
    },
  ];

  return (
    <section className="sa-section">
      <div className="sa-sectionhead">
        <h3 className="sa-sectiontitle">Engagement</h3>
        <DateRangeControl range={range} onChange={setRange} />
      </div>
      <div className="sa-engagementbody">
        <div className="sa-engagementtiles">
          <MetricTile
            label="Avg. Playtime"
            tip={TIP.avgPlaytime}
            extra={w ? <DeltaChip pct={playtimeDelta} /> : undefined}
          >
            {w ? <MinutesValue seconds={w.medianActiveTimeS} /> : NO_DATA}
          </MetricTile>
          <MetricTile label="Avg. Session" tip={TIP.avgSession}>
            {w ? <MinutesValue seconds={w.avgSessionActiveTimeS} /> : NO_DATA}
          </MetricTile>
          <MetricTile label="AFK Time" tip={TIP.afkTime}>
            {w ? formatPercent(w.afkTimePct) : NO_DATA}
          </MetricTile>
        </div>
        <div className="sa-socialchart">
          <span className="sa-charttitle">Social Interactions</span>
          <AnalyticsChart
            series={socialSeries}
            ariaLabel="Social Interactions"
            legend
          />
        </div>
      </div>
    </section>
  );
}

function BackToList({ onBack }: { onBack?: () => void }) {
  return (
    <button
      type="button"
      className="sa-back"
      aria-label="All scenes"
      onClick={onBack}
    >
      {ChevronLeftGlyph}
    </button>
  );
}

function NoSceneData({ onBack }: { onBack?: () => void }) {
  return (
    <>
      <div className="sa-header">
        <div className="sa-headertitle">
          <BackToList onBack={onBack} />
          <h1 className="sa-title">Analytics</h1>
        </div>
      </div>
      <EmptyState
        className="sa-empty"
        icon={ChartEmptyGlyph}
        iconWash
        title="No analytics for this scene yet"
        subtitle="Publish your scene and check back once players have visited. Data updates daily."
      />
    </>
  );
}

function SceneAnalytics({
  scene,
  asOf,
  thumbnail,
  access,
  editHref,
  rankingEnabled,
  realmBaseUrl,
  onBack,
  onExportCsv,
}: {
  scene: SceneStats;
  asOf: string;
  thumbnail?: string;
  access?: "public" | "private";
  editHref?: string | null;
  rankingEnabled: boolean;
  realmBaseUrl?: string;
  onBack?: () => void;
  onExportCsv?: (scene: SceneStats) => void;
}) {
  const handleExport = useCallback(() => {
    downloadSceneCsv(scene, asOf);
    onExportCsv?.(scene);
  }, [scene, asOf, onExportCsv]);

  return (
    <>
      <div className="sa-header">
        <div className="sa-headertitle">
          <BackToList onBack={onBack} />
          <h1 className="sa-title">Analytics - {sceneDisplayName(scene)}</h1>
        </div>
        <button type="button" className="sa-export" onClick={handleExport}>
          {DownloadGlyph}
          Export Analytics
        </button>
      </div>
      <div className="sa-body">
        <div className="sa-rail">
          <SceneCard
            scene={scene}
            thumbnail={thumbnail}
            access={access}
            editHref={editHref}
            realmBaseUrl={realmBaseUrl}
          />
        </div>
        <div className="sa-columns">
          <OverviewSection
            scene={scene}
            asOf={asOf}
            rankingEnabled={rankingEnabled}
          />
          <RetentionSection scene={scene} />
          <EngagementSection scene={scene} asOf={asOf} />
        </div>
      </div>
    </>
  );
}

export default function SceneAnalyticsView({
  phase,
  error = null,
  scenes = [],
  asOf = null,
  selected = null,
  rankingEnabled = false,
  creatorHref = "/creator-hub/metrics",
  operatorHref = "/creator-hub/operator-metrics",
  thumbnails = {},
  worldAccess = {},
  realmBaseUrl,
  editHrefFor,
  onConnect,
  onRetry,
  onOpenScene,
  onBack,
  onExportCsv,
}: SceneAnalyticsViewProps) {
  const activeScene = useMemo(
    () =>
      selected
        ? scenes.find(
            (scene) =>
              scene.sceneType === selected.sceneType &&
              scene.sceneId === selected.sceneId,
          )
        : undefined,
    [scenes, selected],
  );

  const renderBody = () => {
    if (phase === "signed-out") {
      return (
        <EmptyState
          className="sa-empty"
          icon={ChartEmptyGlyph}
          iconWash
          title="Sign in to view your scene metrics"
          subtitle="Visits, retention, and engagement for every scene you publish will appear here."
          actions={[{ label: "Sign In", onClick: onConnect }]}
        />
      );
    }
    if (phase === "loading") {
      return (
        <div className="sa-loading" role="status" aria-live="polite">
          <Spinner aria-hidden="true" />
          <p>Loading your analytics&#x2026;</p>
        </div>
      );
    }
    if (phase === "error") {
      return (
        <div className="sa-error" role="alert">
          <div>
            <p className="sa-errortitle">Could not load metrics</p>
            {error && <p className="sa-errormessage">{error}</p>}
          </div>
          <button type="button" onClick={onRetry}>
            Retry
          </button>
        </div>
      );
    }

    if (selected) {
      return activeScene && asOf ? (
        <SceneAnalytics
          scene={activeScene}
          asOf={asOf}
          thumbnail={thumbnails[sceneKey(activeScene)]}
          access={
            activeScene.sceneType === "world"
              ? worldAccess[activeScene.sceneId]
              : "public"
          }
          editHref={editHrefFor?.(activeScene) ?? null}
          rankingEnabled={rankingEnabled}
          realmBaseUrl={realmBaseUrl}
          onBack={onBack}
          onExportCsv={onExportCsv}
        />
      ) : (
        <NoSceneData onBack={onBack} />
      );
    }

    if (!scenes.length || !asOf) {
      return (
        <>
          <h1 className="sa-title">Analytics</h1>
          <EmptyState
            className="sa-empty"
            icon={ChartEmptyGlyph}
            iconWash
            title="No Places to analyse yet"
            subtitle="Publish a scene to start tracking its performance. Once it's live, you'll find key insights and metrics about your Place here."
          />
        </>
      );
    }

    return (
      <ScenePortfolio
        scenes={scenes}
        thumbnails={thumbnails}
        asOf={asOf}
        onOpen={(scene) => onOpenScene?.(scene)}
      />
    );
  };

  return (
    <div className="sa">
      <div className="cm__tabs">
        <a className="cm__tab" href={creatorHref}>
          Creator
        </a>
        <a className="cm__tab" href={operatorHref}>
          Operator
        </a>
        <span className="cm__tab is-active" aria-current="page">
          Scenes
        </span>
      </div>
      {renderBody()}
    </div>
  );
}
