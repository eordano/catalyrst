import type { ComponentType, CSSProperties, ReactNode } from "react";

import SitesChrome from "../../web/frames/SitesChrome";
import EmptyState from "../../components/EmptyState";
import "../../atoms/button.css";
import "../../admin/admin.css";
import "../operator.css";

type OpRange = "1h" | "6h" | "24h";

type OpLinkProps = {
  to: string;
  prefetch?: "intent" | "render" | "none" | "viewport";
  className?: string;
  style?: CSSProperties;
  onClick?: () => void;
  "aria-label"?: string;
  children?: ReactNode;
};

type OpLinkComponent = ComponentType<OpLinkProps>;

type OpOperatorPlace = {
  id: string;
  title: string | null;
  base_position: string;
  user_count: number | null;
  user_visits: number;
  favorites: number;
  like_rate: number | null;
  highlighted: boolean;
  disabled: boolean;
  world: boolean;
  world_name: string | null;
  headcount: number[] | null;
};

type OpOperatorDashboard = {
  owner: string;
  owner_name: string | null;
  places: OpOperatorPlace[];
};

type OpDashboardTotals = {
  placeCount: number;
  totalLivePlayers: number;
  headcountUnreported: number;
  totalVisits: number;
  disabledCount: number;
};

function totals(places: OpOperatorPlace[]): OpDashboardTotals {
  return places.reduce<OpDashboardTotals>(
    (acc, p) => ({
      placeCount: acc.placeCount + 1,
      totalLivePlayers: acc.totalLivePlayers + (p.user_count ?? 0),
      headcountUnreported: acc.headcountUnreported + (p.user_count == null ? 1 : 0),
      totalVisits: acc.totalVisits + p.user_visits,
      disabledCount: acc.disabledCount + (p.disabled ? 1 : 0),
    }),
    {
      placeCount: 0,
      totalLivePlayers: 0,
      headcountUnreported: 0,
      totalVisits: 0,
      disabledCount: 0,
    },
  );
}

function byVisits(places: OpOperatorPlace[]): OpOperatorPlace[] {
  return [...places].sort((a, b) => b.user_visits - a.user_visits);
}

function rangePoints(range: OpRange): number {
  switch (range) {
    case "1h":
      return 2;
    case "6h":
      return 12;
    default:
      return 48;
  }
}

function windowOf(headcount: number[] | null, range: OpRange): number[] | null {
  if (headcount == null) return null;
  const n = rangePoints(range);
  return headcount.length > n ? headcount.slice(-n) : headcount;
}

function likePct(p: OpOperatorPlace): number | null {
  return p.like_rate == null ? null : Math.round(p.like_rate * 100);
}

function placeWhere(p: OpOperatorPlace): string {
  return p.world && p.world_name ? p.world_name : p.base_position;
}

type OpModerationTarget = "scene-bans" | "scene-admins";

function moderationLink(target: OpModerationTarget, placeId: string): string {
  const base =
    target === "scene-bans" ? "/operator/scene-bans" : "/operator/scene-admins";
  return `${base}?place=${encodeURIComponent(placeId)}`;
}

const W = 240;
const H = 36;
const TOTALS_GRID: CSSProperties = { "--adm-col": "150px" } as CSSProperties;
const MOD_GRID: CSSProperties = { "--adm-col": "260px" } as CSSProperties;

function paths(series: number[]): { line: string; area: string } | null {
  if (series.length < 2) return null;
  const max = Math.max(1, ...series);
  const stepX = W / (series.length - 1);
  const y = (v: number) => H - (v / max) * (H - 4) - 2;
  const pts = series.map((v, i) => `${(i * stepX).toFixed(2)},${y(v).toFixed(2)}`);
  const line = `M${pts.join(" L")}`;
  const area = `${line} L${W.toFixed(2)},${H} L0,${H} Z`;
  return { line, area };
}

function HeadcountTrend({
  series,
  label,
}: {
  series: number[] | null;
  label?: string;
}) {
  if (series == null) {
    return (
      <span
        className="adm-dim"
        aria-label={`Headcount history unavailable for ${label ?? "this place"}`}
      >
        no history
      </span>
    );
  }
  const p = paths(series);
  if (!p) {
    return (
      <span className="adm-dim" aria-label={`No headcount history for ${label ?? "this place"}`}>
        no trend yet
      </span>
    );
  }
  const peak = Math.max(...series);
  return (
    <svg
      className="adm-op__spark"
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="none"
      role="img"
      aria-label={`Headcount trend for ${label ?? "this place"}: peak ${peak}`}
    >
      <path className="adm-op__spark-area" d={p.area} />
      <path className="adm-op__spark-line" d={p.line} />
    </svg>
  );
}

function Stat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

type OperatorPlaceSummaryProps = {
  place: OpOperatorPlace;
  range: OpRange;
  onOpen: (placeId: string) => void;
  LinkComponent: OpLinkComponent;
};

function OperatorPlaceSummary({
  place,
  range,
  onOpen,
  LinkComponent,
}: OperatorPlaceSummaryProps) {
  const pct = likePct(place);
  const live = (place.user_count ?? 0) > 0;
  const headcount = place.user_count == null ? "\u2014" : place.user_count;
  const series = windowOf(place.headcount, range);
  const flagged = live || place.highlighted || place.disabled;

  return (
    <LinkComponent
      to={`/places/${encodeURIComponent(place.id)}`}
      prefetch="intent"
      className={"adm-card adm-card--link" + (place.disabled ? " is-disabled" : "")}
      onClick={() => onOpen(place.id)}
      aria-label={`${place.title || place.id} \u{2014} operator summary`}
    >
      <div className="adm-card__head">
        <span className="adm-card__title u-truncate">{place.title || place.id}</span>
        <span className="adm-mono adm-dim">{placeWhere(place)}</span>
      </div>

      {flagged && (
        <div className="adm-pills">
          {live && (
            <span className="adm-status" data-tone="ok">
              {place.user_count} live
            </span>
          )}
          {place.highlighted && (
            <span className="adm-status" data-tone="info">
              Featured
            </span>
          )}
          {place.disabled && (
            <span className="adm-status" data-tone="bad">
              Disabled
            </span>
          )}
        </div>
      )}

      <dl className="adm-stats">
        <Stat label="Live" value={headcount} />
        <Stat label="Visits" value={place.user_visits.toLocaleString()} />
        <Stat label="Like rate" value={pct == null ? "\u{2014}" : `${pct}%`} />
        <Stat label="Favorites" value={place.favorites.toLocaleString()} />
      </dl>

      <HeadcountTrend series={series} label={place.title || place.id} />
    </LinkComponent>
  );
}

type PlaceVisitTableProps = {
  places: OpOperatorPlace[];
  range: OpRange;
  onOpen: (placeId: string) => void;
  LinkComponent: OpLinkComponent;
};

function PlaceVisitTable({ places, range, onOpen, LinkComponent }: PlaceVisitTableProps) {
  const ranked = byVisits(places);

  return (
    <div className="adm-scroll">
      <table className="adm-table">
        <thead>
          <tr>
            <th className="is-center">#</th>
            <th>Place</th>
            <th className="is-num">Visits</th>
            <th className="is-num">Live</th>
            <th className="is-num">Like rate</th>
            <th className="adm-op__trend">Trend</th>
          </tr>
        </thead>
        <tbody>
          {ranked.length === 0 ? (
            <tr>
              <td className="is-empty" colSpan={6}>
                No operated places yet.
              </td>
            </tr>
          ) : (
            ranked.map((p, i) => {
              const pct = likePct(p);
              return (
                <tr key={p.id}>
                  <td className="is-center">{i + 1}</td>
                  <td>
                    <div className="adm-pills">
                      <LinkComponent
                        to={`/places/${encodeURIComponent(p.id)}`}
                        prefetch="intent"
                        onClick={() => onOpen(p.id)}
                        className="adm-link"
                      >
                        {p.title || p.id}
                      </LinkComponent>
                      <span className="adm-mono adm-dim">{placeWhere(p)}</span>
                      {p.disabled && (
                        <span className="adm-status" data-tone="bad">
                          Disabled
                        </span>
                      )}
                    </div>
                  </td>
                  <td className="is-num">{p.user_visits.toLocaleString()}</td>
                  <td className="is-num">{p.user_count == null ? "\u2014" : p.user_count}</td>
                  <td className="is-num">{pct == null ? "\u{2014}" : `${pct}%`}</td>
                  <td className="adm-op__trend">
                    <HeadcountTrend series={windowOf(p.headcount, range)} label={p.title || p.id} />
                  </td>
                </tr>
              );
            })
          )}
        </tbody>
      </table>
    </div>
  );
}

type ModerationLoadCardProps = {
  place: OpOperatorPlace;
  onModerationLink: (placeId: string, target: OpModerationTarget) => void;
  LinkComponent: OpLinkComponent;
};

function ModerationLoadCard({
  place,
  onModerationLink,
  LinkComponent,
}: ModerationLoadCardProps) {
  return (
    <div className="adm-card">
      <div className="adm-card__title u-truncate">{place.title || place.id}</div>

      <p className="adm-card__text">
        Ban and admin counts are not published by the places API, so none are
        shown here rather than shown as zero.
      </p>

      <div className="adm-actions adm-actions--start">
        <LinkComponent
          to={moderationLink("scene-bans", place.id)}
          prefetch="intent"
          className="btn btn--secondary btn--sm"
          onClick={() => onModerationLink(place.id, "scene-bans")}
        >
          Manage bans
        </LinkComponent>
        <LinkComponent
          to={moderationLink("scene-admins", place.id)}
          prefetch="intent"
          className="btn btn--secondary btn--sm"
          onClick={() => onModerationLink(place.id, "scene-admins")}
        >
          Manage admins
        </LinkComponent>
      </div>
    </div>
  );
}

function Total({ n, label }: { n: number | string; label: string }) {
  return (
    <div className="adm-card">
      <div className="adm-kpi__n">{n}</div>
      <div className="adm-kpi__l">{label}</div>
    </div>
  );
}

function RangeToggle({
  range,
  onSelect,
}: {
  range: OpRange;
  onSelect: (r: OpRange) => void;
}) {
  const opts: OpRange[] = ["1h", "6h", "24h"];
  return (
    <div className="adm-pills" role="tablist" aria-label="Headcount time range">
      {opts.map((r) => (
        <button
          key={r}
          type="button"
          role="tab"
          aria-selected={r === range}
          className={"adm-pill" + (r === range ? " is-active" : "")}
          onClick={() => onSelect(r)}
        >
          {r}
        </button>
      ))}
    </div>
  );
}

type OpDashboardPageProps = {
  range: OpRange;
  dashboard: OpOperatorDashboard;
  viewedAddress: string;
  isDemo: boolean;
  unavailableReason: string | null;
  LinkComponent: OpLinkComponent;
  onSelectRange: (next: OpRange) => void;
  onOpenPlace: (placeId: string) => void;
  onModerationLink: (placeId: string, target: OpModerationTarget) => void;
};

export default function OpDashboardPage({
  range,
  dashboard,
  viewedAddress,
  isDemo,
  unavailableReason,
  LinkComponent,
  onSelectRange,
  onOpenPlace,
  onModerationLink,
}: OpDashboardPageProps) {
  const t = totals(dashboard.places);

  const cards = dashboard.places;
  const ranked = byVisits(dashboard.places);
  const modPlaces = ranked.filter((p) => p.disabled);

  return (
    <SitesChrome active="create" signedIn>
      <div className="adm">
        <div className="adm__page">
          <div className="adm__inner">
            <div className="adm__head">
              <div>
                <h1 className="adm__title">Operator dashboard</h1>
                <p className="adm__sub">
                  Visits, headcount trend and moderation load for the places
                  registered to one address. This is public data &#x2014; the place list
                  (<code>GET /places/api/places?owner=</code>) is unauthenticated,
                  and the address below is a filter, not a claim about who you are.
                </p>
                <p className="adm__sub adm-mono">
                  Viewing places for {dashboard.owner_name ? `${dashboard.owner_name} \u{B7} ` : ""}
                  {viewedAddress}{" "}
                  {isDemo && (
                    <span className="adm-status" data-tone="warn">
                      demo address, not you
                    </span>
                  )}
                </p>
              </div>
              <RangeToggle range={range} onSelect={onSelectRange} />
            </div>

            {unavailableReason ? (
              <div className="adm-notice" data-tone="bad" role="alert">
                <p>
                  The public place list could not be read: {unavailableReason}. No
                  figures are shown, because an empty dashboard and a failed read are
                  not the same thing.
                </p>
              </div>
            ) : cards.length === 0 ? (
              <EmptyState
                variant="inline"
                titleAs="p"
                title={`No places are registered to ${viewedAddress}.`}
              />
            ) : (
              <>
                <div className="adm-grid" style={TOTALS_GRID}>
                  <Total n={t.placeCount} label="Places" />
                  <Total n={t.totalLivePlayers} label="Live players" />
                  <Total n={t.totalVisits.toLocaleString()} label="Visits" />
                  <Total n={t.disabledCount} label="Disabled" />
                </div>
                {t.headcountUnreported > 0 && (
                  <p className="adm__sub">
                    {t.headcountUnreported} of these places reported no headcount, so
                    they are not counted in the live-player total.
                  </p>
                )}

                <h2 className="adm__h2">Per-place summary</h2>
                <div className="adm-grid">
                  {cards.map((p) => (
                    <OperatorPlaceSummary
                      key={p.id}
                      place={p}
                      range={range}
                      onOpen={onOpenPlace}
                      LinkComponent={LinkComponent}
                    />
                  ))}
                </div>

                <h2 className="adm__h2">Operated places by visits</h2>
                <PlaceVisitTable
                  places={dashboard.places}
                  range={range}
                  onOpen={onOpenPlace}
                  LinkComponent={LinkComponent}
                />

                <h2 className="adm__h2">Moderation load</h2>
                <p className="adm__sub">
                  The places API publishes no ban or admin counts, so this section
                  can only list places that are disabled.
                </p>
                {modPlaces.length === 0 ? (
                  <p className="adm__sub">No place here is disabled.</p>
                ) : (
                  <div className="adm-grid" style={MOD_GRID}>
                    {modPlaces.map((p) => (
                      <ModerationLoadCard
                        key={p.id}
                        place={p}
                        onModerationLink={onModerationLink}
                        LinkComponent={LinkComponent}
                      />
                    ))}
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      </div>
    </SitesChrome>
  );
}
