import type { CSSProperties } from "react";
import EmptyState from "../../components/EmptyState";
import Spinner from "../../atoms/Spinner";
import DatumBadge from "../components/DatumBadge";
import DatumNote from "../components/DatumNote";
import { NO_VALUE, showable, type Datum } from "../lib/datum";
import "./metricsdashboardview.css";

const WalletGlyph = (
  <svg
    viewBox="0 0 24 24"
    width="34"
    height="34"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.6"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d="M3 7a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v1" />
    <path d="M3 7v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-6a2 2 0 0 0-2-2H5a2 2 0 0 1-2-2Z" />
    <circle cx="16.5" cy="13" r="1.2" fill="currentColor" stroke="none" />
  </svg>
);

const LOAD_ERROR: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 16,
  flexWrap: "wrap",
  marginBottom: 32,
  border: "1px solid color-mix(in srgb, var(--error) 35%, transparent)",
  background: "color-mix(in srgb, var(--error) 12%, transparent)",
  color: "color-mix(in srgb, var(--error) 45%, var(--text))",
  borderRadius: "var(--r-card)",
  padding: "12px 16px",
  fontSize: 14,
  lineHeight: 1.5,
};
const LOAD_ERROR_RETRY: CSSProperties = {
  flex: "0 0 auto",
  border: "1px solid color-mix(in srgb, var(--error) 45%, transparent)",
  background: "color-mix(in srgb, var(--error) 16%, transparent)",
  color: "inherit",
  borderRadius: "var(--r-control)",
  padding: "6px 14px",
  fontSize: 13,
  fontWeight: 600,
  cursor: "pointer",
};

const ChartGlyph = (
  <svg
    viewBox="0 0 24 24"
    width="34"
    height="34"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.6"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d="M4 20V10" />
    <path d="M10 20V4" />
    <path d="M16 20v-6" />
    <path d="M2 20h20" />
  </svg>
);

type CmSummaryCard = {
  label: string;
  value: string;
  note?: string;
  /**
   * Legacy flag, kept so existing call sites compile unchanged. When `datum`
   * is supplied it wins: availability is derived from provenance, not asserted.
   */
  unavailable?: boolean;
  /**
   * Provenance for this card. Supplying it makes the card speak the shared
   * vocabulary -- badge, endpoint, reason -- instead of a bare boolean.
   */
  datum?: Datum<number | string>;
};

type CmSceneRow = {
  title: string;
  location: string;
  href: string | null;
  visits30d: number | null;
  liveNow: number | null;
};

type MetricsDashboardViewProps = {
  windowDays?: number;
  noAddress?: boolean;
  rescoping?: boolean;
  loadError?: boolean;
  retrying?: boolean;
  empty?: boolean;
  cards?: CmSummaryCard[] | null;
  sceneRows?: CmSceneRow[];
  operatorHref?: string;
  scenesHref?: string;
  onConnect?: () => void;
  onRetry?: () => void;
};

export default function MetricsDashboardView({
  windowDays = 7,
  noAddress = true,
  rescoping = false,
  loadError = false,
  retrying = false,
  empty = false,
  cards = null,
  sceneRows = [],
  operatorHref = "/creator-hub/operator-metrics",
  scenesHref = "/creator-hub/scene-analytics",
  onConnect,
  onRetry,
}: MetricsDashboardViewProps) {
  return (
    <div className="cm">
      <div className="cm__tabs">
        <span className="cm__tab is-active" aria-current="page">
          Creator
        </span>
        <a className="cm__tab" href={operatorHref}>
          Network
        </a>
        <a className="cm__tab" href={scenesHref}>
          Scenes
        </a>
      </div>

      <div className="cm__head">
        <div>
          <h1 className="cm__title">Metrics</h1>
          <p className="cm__sub">
            Your published collections, storefront sales (last {windowDays}{" "}
            days), and scene visits.
          </p>
        </div>
      </div>

      {noAddress ? (
        rescoping ? (
          <div
            className="cm__empty"
            style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              justifyContent: "center",
              gap: 14,
              padding: "56px 24px",
            }}
            role="status"
            aria-live="polite"
          >
            <Spinner aria-hidden="true" />
            <p style={{ margin: 0, color: "var(--ink-45)", fontSize: 14 }}>
              Loading your metrics&#x2026;
            </p>
          </div>
        ) : (
          <EmptyState
            className="cm__empty"
            icon={WalletGlyph}
            iconWash
            title="Sign in to see your creator metrics"
            subtitle="Your published collections, sales, and scene visits will appear here."
            actions={[{ label: "Sign in", onClick: onConnect }]}
            variant={undefined}
            tone={undefined}
            actionsGap={undefined}
            style={undefined}
          />
        )
      ) : (
        <>
          {loadError ? (
            <div className="cm__loaderror" role="alert" style={LOAD_ERROR}>
              <span>Couldn&#x2019;t load your metrics right now.</span>
              <button
                type="button"
                onClick={onRetry}
                disabled={retrying}
                style={LOAD_ERROR_RETRY}
              >
                {retrying ? "Retrying\u{2026}" : "Try again"}
              </button>
            </div>
          ) : empty ? (
            <EmptyState
              className="cm__empty"
              icon={ChartGlyph}
              iconWash
              title="Nothing published yet"
              subtitle={"You haven\u{2019}t published a collection or a scene, or listed an item with this wallet \u{2014} there\u{2019}s no performance data to show yet."}
              actions={[
                { label: "Create a collection", href: "/create/wearables" },
                {
                  label: "Build a scene",
                  href: "/create/scenes",
                  variant: "outline",
                },
              ]}
              variant={undefined}
              tone={undefined}
              actionsGap={undefined}
              style={undefined}
            />
          ) : (
            <div className="cm__cards">
              {(cards ?? []).map((c, i) => (
                <SummaryCard
                  key={i}
                  label={c.label}
                  value={c.value}
                  note={c.note}
                  unavailable={c.unavailable}
                  datum={c.datum}
                />
              ))}
            </div>
          )}

          {!loadError && !empty && sceneRows.length > 0 && (
            <section className="cm__scenes" aria-label="Visits per scene">
              <h2 className="cm__scenestitle">Visits per scene (30d)</h2>
              <table className="cm__scenetable">
                <thead>
                  <tr>
                    <th scope="col">Scene</th>
                    <th scope="col">Location</th>
                    <th scope="col" className="cm__num">Visits (30d)</th>
                    <th scope="col" className="cm__num">In scene now</th>
                  </tr>
                </thead>
                <tbody>
                  {sceneRows.map((r, i) => (
                    <tr key={i}>
                      <td>
                        {r.href ? (
                          <a href={r.href} target="_blank" rel="noreferrer">
                            {r.title}
                          </a>
                        ) : (
                          r.title
                        )}
                      </td>
                      <td className="cm__loc">{r.location || "\u{2014}"}</td>
                      <td className="cm__num">
                        {r.visits30d === null ? (
                          <span className="cm__na">Not available</span>
                        ) : (
                          r.visits30d.toLocaleString("en-US")
                        )}
                      </td>
                      <td className="cm__num">
                        {r.liveNow === null ? (
                          <span className="cm__na">Not available</span>
                        ) : (
                          r.liveNow.toLocaleString("en-US")
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}

          {!empty && (
            <p className="cm__sim" role="note">
              Every number here is scoped to your wallet: published collections
              from the builder server, on-sale items from your own catalog
              listings, sales over the window counted from /market/v1/sales
              filtered to your collection contracts (a real per-creator count +
              MANA volume, not the marketplace-wide feed), and scene visits
              from the places API filtered to scenes you published.
            </p>
          )}
        </>
      )}
    </div>
  );
}

type SummaryCardProps = {
  label: string;
  value: string;
  note?: string;
  unavailable?: boolean;
  datum?: Datum<number | string>;
};

function SummaryCard({
  label,
  value,
  note,
  unavailable,
  datum,
}: SummaryCardProps) {
  // Provenance wins over the boolean: a card cannot claim a value that its
  // datum says never arrived.
  const absent = datum ? !showable(datum) : Boolean(unavailable);
  const shown = datum && !showable(datum) ? NO_VALUE : value;

  return (
    <div className="cm__card">
      <div className="cm__cardlabelrow">
        <p className="cm__cardlabel">{label}</p>
        {datum ? <DatumBadge datum={datum} /> : null}
      </div>
      <div
        className="cm__cardvalue"
        style={
          absent
            ? { fontSize: 16, fontWeight: 600, color: "var(--ink-45)" }
            : undefined
        }
      >
        {shown}
      </div>
      {note && <p className="cm__cardnote">{note}</p>}
      {datum ? <DatumNote datum={datum} /> : null}
    </div>
  );
}
