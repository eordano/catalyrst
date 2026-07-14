import { CreatorHubChromeMaybe } from "../frames/CreatorHubChrome";
import "./operatormetricsview.css";

type PresenceSource = "catalyst" | "unavailable";
type OpMetricsSource = "live" | "unavailable";

type SourceBadgeProps = { source: PresenceSource };

function SourceBadge({ source }: SourceBadgeProps) {
  const live = source === "catalyst";
  return (
    <span className={"op-badge" + (live ? " op-badge--live" : "")}>
      {live ? "LIVE presence" : "Presence unavailable"}
    </span>
  );
}

type MetricsBadgeProps = { source: OpMetricsSource };

function MetricsBadge({ source }: MetricsBadgeProps) {
  if (source === "live") {
    return (
      <span
        className="op-badge op-badge--live"
        title="Live counts from the telemetry store"
      >
        LIVE
      </span>
    );
  }
  return (
    <span className="op-sample" title={"This hub isn\u{2019}t connected to a telemetry store"}>
      Not connected
    </span>
  );
}

type KpiProps = {
  label: string;
  value: number;
  sub?: string;
  accent?: string;
};

function Kpi({ label, value, sub, accent }: KpiProps) {
  return (
    <div className="op-kpi">
      <div className="op-kpiValue" style={accent ? { color: accent } : undefined}>
        {value}
      </div>
      <div className="op-kpiLabel">{label}</div>
      {sub && <div className="op-kpiSub">{sub}</div>}
    </div>
  );
}

type OpBarRow = { key: string; label: string; meta: string; count: number; href: string };

type OccupancyBarsProps = { rows: OpBarRow[]; openLabel: string };

function OccupancyBars({ rows, openLabel }: OccupancyBarsProps) {
  const max = Math.max(1, ...rows.map((r) => r.count));
  return (
    <ul className="op-bars">
      {rows.map((r) => (
        <li key={r.key} className="op-barRow">
          <div className="op-barHead">
            <span className="op-barLabel">{r.label}</span>
            {r.meta && <code className="op-barMeta">{r.meta}</code>}
          </div>
          <div className="op-barLine">
            <div className="op-barTrack">
              <div
                className="op-barFill"
                style={{ width: `${Math.round((r.count / max) * 100)}%` }}
              />
            </div>
            <span className="op-barCount">{r.count}</span>
          </div>
          <div className="op-barActions">
            <a href={r.href} target="_blank" rel="noreferrer" className="op-linkSm">
              {openLabel}
            </a>
          </div>
        </li>
      ))}
    </ul>
  );
}

type OpTotals = {
  peers: number;
  scenes: number;
  worlds: number;
  sceneUsers: number;
  worldUsers: number;
  activeScenes: number;
  activeWorlds: number;
};
type OpFunnelStage = { event: string; label: string; count: number };
type OpFunnel = { stages: OpFunnelStage[]; conversion: number; source: OpMetricsSource };
type OpAdminRow = { event: string; label: string; count: number };
type OpAdmin = { rows: OpAdminRow[]; source: OpMetricsSource };

type OperatorMetricsViewProps = {
  source?: PresenceSource;
  totals?: OpTotals;
  funnel?: OpFunnel;
  admin?: OpAdmin;
  sceneRows?: OpBarRow[];
  worldRows?: OpBarRow[];
  guardrailEvent?: string;
  signedIn?: boolean;
  chrome?: boolean;
  account?: string;
  name?: string;
  onSignIn?: () => void;
  onFunnelTab?: () => void;
  onFunnelClick?: (target: string) => void;
};

export default function OperatorMetricsView({
  source = "unavailable",
  totals = {
    peers: 0,
    scenes: 0,
    worlds: 0,
    sceneUsers: 0,
    worldUsers: 0,
    activeScenes: 0,
    activeWorlds: 0,
  },
  funnel = { stages: [], conversion: 0, source: "unavailable" },
  admin = { rows: [], source: "unavailable" },
  sceneRows = [],
  worldRows = [],
  guardrailEvent = "",
  signedIn = false,
  account = "",
  name = "",
  onSignIn,
  onFunnelTab,
  onFunnelClick,
  chrome = true,
}: OperatorMetricsViewProps) {
  const unavailable = source !== "catalyst";
  const rowsHavePeople =
    sceneRows.some((r) => r.count > 0) || worldRows.some((r) => r.count > 0);
  const liveEmpty =
    !unavailable &&
    !rowsHavePeople &&
    totals.peers === 0 &&
    totals.activeScenes === 0 &&
    totals.activeWorlds === 0;

  return (
    <div className="creator-hub-operator-metrics">
      <CreatorHubChromeMaybe
      chrome={chrome}
        active="metrics"
        signedIn={signedIn}
        account={account}
        name={name}
        onSignIn={onSignIn}
      >
        <div className="op-page ui2">
          <header className="op-head">
            <div className="cm__tabs">
              <a
                className="cm__tab"
                href="/creator-hub/metrics"
                onClick={(e) => {
                  if (onFunnelTab) {
                    e.preventDefault();
                    onFunnelTab();
                  }
                }}
              >
                Creator
              </a>
              <span className="cm__tab is-active" aria-current="page">
                Network
              </span>
              <a className="cm__tab" href="/creator-hub/scene-analytics">
                Scenes
              </a>
            </div>
            <h1 className="op-h1">Network metrics</h1>
            <p className="op-sub">
              Network-wide live occupancy, plus the deploy funnel and moderation
              activity &#x2014; none of it scoped to your scenes.
            </p>
            <SourceBadge source={source} />
          </header>

          <p className="op-eyebrow">Live occupancy</p>
          {unavailable ? (
            <section aria-label="Live occupancy">
              <p className="op-empty">
                Couldn&#x2019;t reach the presence API &#x2014; live occupancy can&#x2019;t be shown
                right now. Try again shortly.
              </p>
            </section>
          ) : liveEmpty ? (
            <section aria-label="Live occupancy">
              <p className="op-empty">No one is in-world right now.</p>
            </section>
          ) : (
            <section aria-label="Live occupancy" className="op-kpiRow">
              <Kpi
                label="Players online"
                value={totals.peers}
                accent="var(--brand)"
              />
              <Kpi
                label="Active scenes"
                value={totals.activeScenes}
                sub={`${totals.sceneUsers} in scenes`}
              />
              <Kpi
                label="Active worlds"
                value={totals.activeWorlds}
                sub={`${totals.worldUsers} in worlds`}
              />
            </section>
          )}

          <div className="op-cols">
            <section className="op-card" aria-label="Per-scene occupancy">
              <h2 className="op-h2">
                Scene occupancy{" "}
                <span className="op-count">({sceneRows.length})</span>
              </h2>
              {unavailable ? (
                <p className="op-empty">Presence data unavailable.</p>
              ) : sceneRows.length === 0 ? (
                <p className="op-empty">No occupied scenes in the latest snapshot.</p>
              ) : (
                <OccupancyBars rows={sceneRows} openLabel="Scene-admin" />
              )}
            </section>

            <section className="op-card" aria-label="Per-world occupancy">
              <h2 className="op-h2">
                World occupancy{" "}
                <span className="op-count">({worldRows.length})</span>
              </h2>
              {unavailable ? (
                <p className="op-empty">Presence data unavailable.</p>
              ) : worldRows.length === 0 ? (
                <p className="op-empty">No active worlds in the latest snapshot.</p>
              ) : (
                <OccupancyBars rows={worldRows} openLabel="Jump in" />
              )}
            </section>
          </div>

          <details open={funnel.source === "live"}>
            <summary className="op-eyebrow op-eyebrow--scaffold">
              {funnel.source === "live"
                ? "Deploy funnel & moderation activity"
                : "Deploy funnel & moderation (telemetry not connected)"}
            </summary>
            <div className="op-cols">
              <section className="op-card" aria-label="Deploy funnel">
                <h2 className="op-h2">
                  Deploy funnel <MetricsBadge source={funnel.source} />{" "}
                  <span className="op-convNote">
                    {Math.round(funnel.conversion * 100)}% completed / started
                  </span>
                </h2>
                <ul className="op-funnel">
                  {funnel.stages.map((st) => {
                    const top = funnel.stages[0]?.count || 1;
                    const pct = Math.round((st.count / top) * 100);
                    const guardrail = st.event === guardrailEvent;
                    return (
                      <li key={st.event} className="op-funnelRow">
                        <div className="op-funnelLabel">
                          <span>{st.label}</span>
                          <code className="op-event">{st.event}</code>
                        </div>
                        <div className="op-barTrack">
                          <div
                            className="op-barFill"
                            style={
                              guardrail
                                ? { width: `${pct}%`, background: "var(--gold)" }
                                : { width: `${pct}%` }
                            }
                          />
                        </div>
                        <span className="op-funnelCount">{st.count}</span>
                      </li>
                    );
                  })}
                </ul>
                <div className="op-ctaRow">
                  <a
                    href="/creator-hub/deploy-world?from=operator"
                    className="op-cta"
                    onClick={() => onFunnelClick && onFunnelClick("world")}
                  >
                    Deploy to World &#x2192;
                  </a>
                </div>
                <p className="op-note">
                  {funnel.source === "live"
                    ? "Live counts aggregated from the operator_deploy_* telemetry events the deploy wizards emit at their call sites."
                    : "This hub isn\u{2019}t connected to a telemetry store yet, so counts show 0. Connect it to the catalyst-telemetry store to surface the live deploy funnel."}
                </p>
              </section>

              <section className="op-card" aria-label="Moderation activity">
                <h2 className="op-h2">
                  Moderation activity <MetricsBadge source={admin.source} />
                </h2>
                <ul className="op-adminList">
                  {admin.rows.map((r) => (
                    <li key={r.event} className="op-adminRow">
                      <span className="op-adminCount">{r.count}</span>
                      <span className="op-adminLabel">{r.label}</span>
                      <code className="op-event">{r.event}</code>
                    </li>
                  ))}
                </ul>
                <p className="op-note">
                  {admin.source === "live"
                    ? "Live counts from the operator_scene_admin_opened / operator_admin_changed / operator_ban_issued events emitted by the scene-admin + bans surfaces."
                    : "This hub isn\u{2019}t connected to a telemetry store yet, so counts show 0. Connect it to the catalyst-telemetry store to surface live moderation activity."}
                </p>
              </section>
            </div>
          </details>
        </div>
      </CreatorHubChromeMaybe>
    </div>
  );
}
