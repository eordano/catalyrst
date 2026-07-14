import { useEffect, useRef } from "react";
import { useNavigate } from "react-router";

import { REDIRECT_DASHBOARD, toDashboardRows, totalRedirects, pct, fmtCount } from "@data/lib/catalyst/creator-hub/builder-redirect";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/builder.redirects-metrics";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "creator-hub/integration-redirect-item-publish-curate";
const DASHBOARD_VIEWED = "creator_builder_redirect_dashboard_viewed";

const FALLBACK: Assignment = {
  variant: "redirect",
  flags: { redirect: true, status: 308 },
  experimentKey: "creator_builder_redirect",
};

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const fx = REDIRECT_DASHBOARD;
  const rows = toDashboardRows(fx);

  const payload = {
    sid,
    windowDays: fx.windowDays,
    redirectStatus: fx.redirectStatus,
    total: totalRedirects(fx),
    rows,
    source: fx._source,
  };

  return wrap(payload);
}

export default function BuilderRedirectsMetricsRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const navigate = useNavigate();

  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(
      DASHBOARD_VIEWED,
      { window_days: d.windowDays, total: d.total },
      { sid: d.sid, story: STORY },
    );
  }, [d.sid, d.windowDays, d.total]);

  return (
    <main style={S.main}>
      <header style={S.head}>
        <h1 style={S.title}>Builder &rarr; Create redirects</h1>
        <p style={S.sub}>
          Legacy <code>/builder</code> creator surfaces now {d.redirectStatus}-redirect
          into the unified Creator Hub under <code>/create</code>. Redirect volume,
          last {d.windowDays} days.
        </p>
      </header>

      <div style={S.cards}>
        <div style={S.card}>
          <p style={S.cardLabel}>Total redirects ({d.windowDays}d)</p>
          <div style={S.cardValue}>{fmtCount(d.total)}</div>
        </div>
        <div style={S.card}>
          <p style={S.cardLabel}>Legacy surfaces redirected</p>
          <div style={S.cardValue}>{d.rows.length}</div>
        </div>
      </div>

      <section style={S.panel} aria-label="Redirect mapping and volume">
        <table style={S.table}>
          <thead>
            <tr>
              <th style={S.th}>Legacy surface</th>
              <th style={S.th}>Unified destination</th>
              <th style={{ ...S.th, textAlign: "right" }}>Redirects</th>
              <th style={{ ...S.th, textAlign: "right" }}>Share</th>
            </tr>
          </thead>
          <tbody>
            {d.rows.map((r) => (
              <tr key={r.from}>
                <td style={S.td}>
                  <code>{r.legacyPath}</code>
                </td>
                <td style={S.td}>
                  <code>{r.to}</code>
                  {r.param && (
                    <span style={S.note}>
                      {" "}
                      (:{r.param.name} &rarr;{" "}
                      {r.param.queryKey ? `?${r.param.queryKey}` : "path"})
                    </span>
                  )}
                </td>
                <td style={{ ...S.td, textAlign: "right" }}>
                  {fmtCount(r.redirects)}
                </td>
                <td style={{ ...S.td, textAlign: "right" }}>
                  <span style={S.barWrap} aria-hidden>
                    <span
                      style={{ ...S.bar, width: `${Math.max(r.share * 100, 2)}%` }}
                    />
                  </span>
                  {pct(r.share)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>

        <p style={S.sim} role="note">
          The mapping table is the real legacy&rarr;unified contract enforced by
          the five redirect routes (query + path params preserved). Per-surface
          counts are <strong>unavailable</strong>:{" "}
          <code>creator_builder_redirect</code> is emitted server-side in each
          redirect loader but there is no SSR read-back path for the telemetry, so
          no volume is shown (no fixture/simulated numbers).
        </p>
      </section>

      <nav style={S.actions} aria-label="Try a redirect">
        <button
          type="button"
          style={S.btn}
          onClick={() => void navigate("/builder/item-editor?collection=demo&step=model")}
        >
          Open legacy /builder/item-editor &rarr;
        </button>
        <button
          type="button"
          style={S.btn}
          onClick={() => void navigate("/builder/item/demo-item-1")}
        >
          Open legacy /builder/item/:id &rarr;
        </button>
      </nav>
    </main>
  );
}

const S: Record<string, React.CSSProperties> = {
  main: { maxWidth: 880, margin: "0 auto", padding: "32px 20px", color: "#f4f2f7" },
  head: { marginBottom: 20 },
  title: { fontSize: 24, fontWeight: 700, margin: 0 },
  sub: { color: "rgba(255,255,255,0.62)", marginTop: 6, lineHeight: 1.5 },
  cards: { display: "flex", gap: 12, marginBottom: 20, flexWrap: "wrap" },
  card: {
    flex: "1 1 200px",
    background: "rgba(255,255,255,0.04)",
    border: "1px solid rgba(255,255,255,0.08)",
    borderRadius: 12,
    padding: "16px 18px",
  },
  cardLabel: { fontSize: 13, color: "rgba(255,255,255,0.62)", margin: 0 },
  cardValue: { fontSize: 28, fontWeight: 700, marginTop: 4 },
  panel: {
    background: "rgba(255,255,255,0.03)",
    border: "1px solid rgba(255,255,255,0.08)",
    borderRadius: 12,
    padding: 18,
  },
  table: { width: "100%", borderCollapse: "collapse", fontSize: 14 },
  th: {
    textAlign: "left",
    padding: "8px 10px",
    borderBottom: "1px solid rgba(255,255,255,0.08)",
    color: "rgba(255,255,255,0.62)",
    fontWeight: 600,
  },
  td: { padding: "10px", borderBottom: "1px solid rgba(255,255,255,0.06)", verticalAlign: "top" },
  note: { color: "rgba(255,255,255,0.45)", fontSize: 12 },
  barWrap: {
    display: "inline-block",
    width: 80,
    height: 6,
    background: "rgba(255,255,255,0.08)",
    borderRadius: 3,
    marginRight: 8,
    verticalAlign: "middle",
    overflow: "hidden",
  },
  bar: { display: "block", height: "100%", background: "#5b3df5", borderRadius: 3 },
  sim: { color: "rgba(255,255,255,0.62)", fontSize: 13, marginTop: 14, lineHeight: 1.5 },
  actions: { display: "flex", gap: 10, marginTop: 18, flexWrap: "wrap" },
  btn: {
    background: "rgba(255,255,255,0.08)",
    color: "#fff",
    border: "none",
    borderRadius: 8,
    padding: "10px 14px",
    fontSize: 14,
    cursor: "pointer",
  },
};
