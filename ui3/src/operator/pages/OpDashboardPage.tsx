import type { ComponentType, CSSProperties, ReactNode } from "react";

import SitesChrome from "../../web/frames/SitesChrome";
import "../../web/pages/stwhatsonadminusers.css";
import "./opdashboardpage.css";

export type OpRange = "1h" | "6h" | "24h";

export type OpLinkProps = {
  to: string;
  prefetch?: "intent" | "render" | "none" | "viewport";
  className?: string;
  style?: CSSProperties;
  onClick?: () => void;
  "aria-label"?: string;
  children?: ReactNode;
};

export type OpLinkComponent = ComponentType<OpLinkProps>;

/**
 * Only fields the places API actually serialises appear here. A 24h visit
 * count, a ban count and an admin count are not among them -- see
 * `catalyrst-places/src/ports/places/rows.rs:43-95` -- so this page no longer
 * has props for them and no longer renders them as zeroes.
 */
export type OpOperatorPlace = {
  id: string;
  title: string | null;
  base_position: string;
  /** Null when the API reported no headcount for this place; not zero players. */
  user_count: number | null;
  user_visits: number;
  favorites: number;
  like_rate: number | null;
  highlighted: boolean;
  disabled: boolean;
  world: boolean;
  world_name: string | null;
  /**
   * Null when no presence history was read for this place, which is not the
   * same as a history of zeroes: the empty sparkline says "sampled, nobody
   * there", so an unread history must say something else.
   */
  headcount: number[] | null;
};

export type OpOperatorDashboard = {
  owner: string;
  owner_name: string | null;
  places: OpOperatorPlace[];
};

export type OpDashboardTotals = {
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
    case "24h":
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

type OpModerationTarget = "scene-bans" | "scene-admins";

function moderationLink(target: OpModerationTarget, placeId: string): string {
  const base =
    target === "scene-bans" ? "/operator/scene-bans" : "/operator/scene-admins";
  return `${base}?place=${encodeURIComponent(placeId)}`;
}

const W = 240;
const H = 36;

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
        className="spark__empty"
        aria-label={`Headcount history unavailable for ${label ?? "this place"}`}
      >
        no history
      </span>
    );
  }
  const p = paths(series);
  if (!p) {
    return (
      <span className="spark__empty" aria-label={`No headcount history for ${label ?? "this place"}`}>
        no trend yet
      </span>
    );
  }
  const peak = Math.max(...series);
  return (
    <svg
      className="spark"
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="none"
      role="img"
      aria-label={`Headcount trend for ${label ?? "this place"}: peak ${peak}`}
    >
      <path className="spark__area" d={p.area} />
      <path className="spark__line" d={p.line} />
    </svg>
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

  return (
    <LinkComponent
      to={`/places/${encodeURIComponent(place.id)}`}
      prefetch="intent"
      className={"opc" + (place.disabled ? " is-disabled" : "")}
      onClick={() => onOpen(place.id)}
      aria-label={`${place.title || place.id} \u{2014} operator summary`}
    >
      <div className="opc__top">
        <span className="opc__title">{place.title || place.id}</span>
        <span className="opc__coords">
          {place.world && place.world_name ? place.world_name : place.base_position}
        </span>
      </div>

      <div className="opc__badges">
        {live && <span className="opc__badge opc__badge--live">{place.user_count} live</span>}
        {place.highlighted && (
          <span className="opc__badge opc__badge--featured">Featured</span>
        )}
        {place.disabled && (
          <span className="opc__badge opc__badge--disabled">Disabled</span>
        )}
      </div>

      <div className="opc__kpis">
        <div>
          <div className="opc__kpi-n">{headcount}</div>
          <div className="opc__kpi-l">Live</div>
        </div>
        <div>
          <div className="opc__kpi-n">{place.user_visits.toLocaleString()}</div>
          <div className="opc__kpi-l">Visits</div>
        </div>
        <div>
          <div className="opc__kpi-n">{pct == null ? "\u{2014}" : `${pct}%`}</div>
          <div className="opc__kpi-l">Like rate</div>
        </div>
        <div>
          <div className="opc__kpi-n">{place.favorites.toLocaleString()}</div>
          <div className="opc__kpi-l">Favorites</div>
        </div>
      </div>

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
    <div className="au__tablewrap">
      <table className="au__table">
        <thead>
          <tr>
            <th className="au-cell au-cell--center op__rank-num">#</th>
            <th className="au-cell">Place</th>
            <th className="au-cell op__num">Visits</th>
            <th className="au-cell op__num">Live</th>
            <th className="au-cell op__num">Like rate</th>
            <th className="au-cell op__rank-spark">Trend</th>
          </tr>
        </thead>
        <tbody>
          {ranked.length === 0 ? (
            <tr>
              <td className="au-cell au-cell--empty" colSpan={6}>
                No operated places yet.
              </td>
            </tr>
          ) : (
            ranked.map((p, i) => {
              const pct = likePct(p);
              return (
                <tr className="au-row" key={p.id}>
                  <td className="au-cell au-cell--center">{i + 1}</td>
                  <td className="au-cell au-cell--user">
                    <LinkComponent
                      to={`/places/${encodeURIComponent(p.id)}`}
                      prefetch="intent"
                      onClick={() => onOpen(p.id)}
                      className="au-cell__name op__rank-name"
                    >
                      {p.title || p.id}
                    </LinkComponent>
                    <span className="au-cell__addr op__rank-coords">
                      {p.world && p.world_name ? p.world_name : p.base_position}
                    </span>
                    {p.disabled && (
                      <span className="opc__badge opc__badge--disabled op__rank-badge">
                        Disabled
                      </span>
                    )}
                  </td>
                  <td className="au-cell op__num">{p.user_visits.toLocaleString()}</td>
                  <td className="au-cell op__num">
                    {p.user_count == null ? "\u2014" : p.user_count}
                  </td>
                  <td className="au-cell op__num">{pct == null ? "\u{2014}" : `${pct}%`}</td>
                  <td className="au-cell op__rank-spark">
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
    <div className="opm">
      <div className="opm__title">{place.title || place.id}</div>

      <p className="opm__unknown">
        Ban and admin counts are not published by the places API, so none are
        shown here rather than shown as zero.
      </p>

      <div className="opm__links">
        <LinkComponent
          to={moderationLink("scene-bans", place.id)}
          prefetch="intent"
          className="opm__link"
          onClick={() => onModerationLink(place.id, "scene-bans")}
        >
          Manage bans
        </LinkComponent>
        <LinkComponent
          to={moderationLink("scene-admins", place.id)}
          prefetch="intent"
          className="opm__link"
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
    <div className="op__total">
      <div className="op__total-n">{n}</div>
      <div className="op__total-l">{label}</div>
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
    <div className="op__range" role="tablist" aria-label="Headcount time range">
      {opts.map((r) => (
        <button
          key={r}
          type="button"
          role="tab"
          aria-selected={r === range}
          className={"op__range-btn" + (r === range ? " is-active" : "")}
          onClick={() => onSelect(r)}
        >
          {r}
        </button>
      ))}
    </div>
  );
}

/**
 * The place list behind this page is a PUBLIC, unauthenticated read:
 * `GET /places/api/places?owner=` --
 * `catalyrst-places/src/handlers/places.rs:66-73` calls `auth_address_optional`
 * and gates nothing. Anyone, signed in or not, gets the same answer.
 *
 * So `?owner=` is a *filter*, never an identity claim and never a permission.
 * This component says "viewing places for <address>", not "your places", and
 * calls out the built-in demo address as not being the viewer. Every
 * privileged control reachable from here (scene admins, scene bans) is
 * unavailable on this node regardless of the address in the URL.
 *
 * `unavailableReason` is a string, not a boolean: an empty dashboard and a
 * dashboard that could not be loaded must not look the same.
 */
export type OpDashboardPageProps = {
  range: OpRange;
  dashboard: OpOperatorDashboard;
  /** The address the public filter was run for. Not an identity claim. */
  viewedAddress: string;
  /** True when the address is the built-in demo value, not the viewer. */
  isDemo: boolean;
  /** Non-null when the public read failed. Renders instead of any figures. */
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
      <div className="op">
        <div className="op__head">
          <div>
            <h1 className="op__title">Operator dashboard</h1>
            <p className="op__sub">
              Visits, headcount trend and moderation load for the places
              registered to one address. This is public data &#x2014; the place list
              (<code>GET /places/api/places?owner=</code>) is unauthenticated,
              and the address below is a filter, not a claim about who you are.
            </p>
            <p className="op__owner">
              Viewing places for {dashboard.owner_name ? `${dashboard.owner_name} \u{B7} ` : ""}
              {viewedAddress}
              {isDemo && (
                <span className="op__degraded"> &#x2014; demo address, not you</span>
              )}
            </p>
          </div>
          <RangeToggle range={range} onSelect={onSelectRange} />
        </div>

        {unavailableReason ? (
          <p className="op__sub" role="alert">
            The public place list could not be read: {unavailableReason}. No
            figures are shown, because an empty dashboard and a failed read are
            not the same thing.
          </p>
        ) : cards.length === 0 ? (
          <p className="op__sub">
            No places are registered to {viewedAddress}.
          </p>
        ) : (
          <>
            <div className="op__totals">
              <Total n={t.placeCount} label="Places" />
              <Total n={t.totalLivePlayers} label="Live players" />
              <Total n={t.totalVisits.toLocaleString()} label="Visits" />
              <Total n={t.disabledCount} label="Disabled" />
            </div>
            {t.headcountUnreported > 0 && (
              <p className="op__sub">
                {t.headcountUnreported} of these places reported no headcount, so
                they are not counted in the live-player total.
              </p>
            )}

            <h2 className="op__section">Per-place summary</h2>
            <div className="op__cards">
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

            <h2 className="op__section">Operated places by visits</h2>
            <PlaceVisitTable
              places={dashboard.places}
              range={range}
              onOpen={onOpenPlace}
              LinkComponent={LinkComponent}
            />

            <h2 className="op__section">Moderation load</h2>
            <p className="op__sub">
              The places API publishes no ban or admin counts, so this section
              can only list places that are disabled.
            </p>
            {modPlaces.length === 0 ? (
              <p className="op__sub">No place here is disabled.</p>
            ) : (
              <div className="op__mod">
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
    </SitesChrome>
  );
}
