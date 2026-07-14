import { useId, useState } from "react";
import EmptyState from "../../components/EmptyState";
import AnalyticsChart from "../components/AnalyticsChart";
import DatumBadge, { DatumTally } from "../components/DatumBadge";
import DatumNote from "../components/DatumNote";
import DatumTile from "../components/DatumTile";
import SourceLedger, {
  type SourceLedgerGroup,
} from "../components/SourceLedger";
import UnbuiltPanel from "../components/UnbuiltPanel";
import {
  NO_ADDRESS_BODY,
  NO_ADDRESS_TITLE,
  NO_VALUE,
  PUBLIC_DATA_DISCLOSURE,
  formatDatum,
  formatReadStamp,
  formatUtcDay,
  noteLines,
  showable,
  tallyStates,
  type Datum,
} from "../lib/datum";
import type { ChartSeries } from "../lib/scene-analytics";
import "./chactivityindex.css";

export type ActivityWorldRow = {
  /** The NAME, e.g. `petbarn.dcl.eth`. */
  name: string;
  title: string | null;
  lastDeployedAt: string | null;
  /** `0` means nothing was ever deployed to this NAME -- a different fact from "empty". */
  deployedScenes: number | null;
  blockedSince: string | null;
  /** Headcount at the last presence snapshot. `no-sample` != zero. */
  now: Datum<number>;
  /**
   * Mandatory whenever `now` is a showable literal `0`: a real zero and a
   * missing sample both render as a small number-shaped thing, and only this
   * sentence tells them apart. e.g. "a real zero -- sampled 2m ago, nobody in".
   */
  nowNote?: string | null;
  peak7d: Datum<number>;
  href: string;
  jumpUrl?: string | null;
  /** Where "Publish a scene here" goes for a never-deployed NAME. */
  publishHref?: string;
};

export type BusiestRow = {
  key: string;
  label: string;
  sub?: string | null;
  count: number;
  href?: string | null;
};

export type ParcelActivity = {
  pointer: string;
  series: ChartSeries[];
  gapBands?: { fromIndex: number; toIndex: number }[];
  peak: Datum<number>;
  /** e.g. "38 of 1 214". Never relabelled "visits". */
  occupied: Datum<string>;
  /** e.g. "2026-07-13 02:15Z". Never a nominal "last 30 days". */
  historyBegins: Datum<string>;
  /** True when the collector has no snapshots at all for this pointer. */
  noHistory?: boolean;
  jumpUrl?: string | null;
};

export type ChActivityIndexPageProps = {
  /** `null` renders the no-address state. There is no demo owner, ever. */
  address: string | null;
  readAt?: string | null;
  peopleInYourWorlds: Datum<number>;
  /** Rendered verbatim, e.g. "22 peers, 8 islands". */
  networkPresence: Datum<string>;
  worlds: Datum<readonly ActivityWorldRow[]>;
  busiestScenes: Datum<readonly BusiestRow[]>;
  busiestWorlds: Datum<readonly BusiestRow[]>;
  /** Result of the `?pointer=x,y` lookup, when there is one. */
  parcel?: ParcelActivity | null;
  parcelPointer?: string | null;
  sources: readonly SourceLedgerGroup[];
  publishHref?: string;
  namesHref?: string;
  dataSourcesHref?: string;
  onRefresh?: () => void;
  refreshing?: boolean;
  onConnect?: () => void;
  onAddressSubmit?: (address: string) => void;
  onPointerLookup?: (pointer: string) => void;
  /** Pinned clock, for deterministic stories and tests. */
  now?: number;
};

const int = (v: number | string) =>
  typeof v === "number" ? v.toLocaleString("en-US") : v;

/** Compact per-row reading: the number (or `--`) and its badge. */
function RowDatum({
  datum,
  note = null,
  now,
}: {
  datum: Datum<number>;
  /** Only rendered for a showable value; the absent states derive their own. */
  note?: string | null;
  now?: number;
}) {
  const lines = noteLines(datum, now);
  return (
    <div className="ai__cell">
      <span
        className={
          showable(datum) ? "ai__cellnum" : "ai__cellnum ai__cellnum--absent"
        }
      >
        {formatDatum(datum, int)}
      </span>
      <DatumBadge datum={datum} now={now} />
      {showable(datum) ? (
        note ? (
          <span className="ai__cellnote">{note}</span>
        ) : null
      ) : (
        <span className="ai__cellnote">{lines.at(-1)}</span>
      )}
    </div>
  );
}

function WorldsTable({
  rows,
  publishHref,
  now,
}: {
  rows: readonly ActivityWorldRow[];
  publishHref: string;
  now?: number;
}) {
  return (
    <div className="ai__scroll" tabIndex={0}>
      <table className="ai__table">
        <thead>
          <tr>
            <th scope="col">World</th>
            <th scope="col">Last deployed</th>
            <th scope="col">Now</th>
            <th scope="col">Peak 7d</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const blocked = row.blockedSince !== null;
            const neverDeployed = row.deployedScenes === 0;
            return (
              <tr key={row.name}>
                <th scope="row" className="ai__worldcell">
                  <a className="ai__worldname" href={row.href}>
                    {row.name}
                  </a>
                  {row.title ? (
                    <span className="ai__worldtitle">&#x201C;{row.title}&#x201D;</span>
                  ) : null}
                  {row.deployedScenes !== null ? (
                    <span className="ai__worldmeta">
                      {row.deployedScenes}{" "}
                      {row.deployedScenes === 1 ? "scene" : "scenes"} deployed
                    </span>
                  ) : null}
                </th>

                <td className="ai__when">
                  {formatUtcDay(row.lastDeployedAt) ?? NO_VALUE}
                </td>

                {blocked ? (
                  <td className="ai__blocked" colSpan={2}>
                    <span className="dv-badge dv-badge--unavailable">
                      <span className="dv-badge__glyph" aria-hidden="true">
                        &#x2298;
                      </span>
                      <span className="dv-badge__word">Blocked</span>
                    </span>
                    <span className="ai__cellnote">
                      Blocked on worlds-content-server since{" "}
                      {formatUtcDay(row.blockedSince) ?? row.blockedSince}.
                    </span>
                  </td>
                ) : neverDeployed ? (
                  <td className="ai__never" colSpan={2}>
                    <span className="dv-badge dv-badge--unbuilt">
                      <span className="dv-badge__glyph" aria-hidden="true">
                        &#x25A8;
                      </span>
                      <span className="dv-badge__word">Never deployed</span>
                    </span>
                    <span className="ai__cellnote">
                      No scene deployed to this NAME, so presence has never had
                      anything to sample.
                    </span>
                    <a
                      className="ai__cellaction"
                      href={row.publishHref ?? publishHref}
                    >
                      Publish a scene here
                    </a>
                  </td>
                ) : (
                  <>
                    <td>
                      <RowDatum
                        datum={row.now}
                        note={row.nowNote ?? null}
                        now={now}
                      />
                    </td>
                    <td>
                      <RowDatum datum={row.peak7d} now={now} />
                    </td>
                  </>
                )}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function BusiestList({
  heading,
  datum,
  now,
}: {
  heading: string;
  datum: Datum<readonly BusiestRow[]>;
  now?: number;
}) {
  return (
    <div className="ai__busiest">
      <div className="ai__busiesthead">
        <h3 className="ai__busiesttitle">{heading}</h3>
        <DatumBadge datum={datum} now={now} />
      </div>
      {showable(datum) ? (
        datum.value.length === 0 ? (
          <p className="ai__busiestempty">
            The last snapshot found nobody anywhere in this realm. That is a
            reading, not a failure.
          </p>
        ) : (
          <ol className="ai__busiestlist">
            {datum.value.map((row) => (
              <li key={row.key}>
                {row.href ? (
                  <a href={row.href}>{row.label}</a>
                ) : (
                  <span>{row.label}</span>
                )}
                {row.sub ? <span className="ai__busiestsub">{row.sub}</span> : null}
                <span className="ai__busiestcount">{row.count}</span>
              </li>
            ))}
          </ol>
        )
      ) : null}
      <DatumNote datum={datum} now={now} />
    </div>
  );
}

/**
 * `/creator-hub/activity` -- your worlds with live headcount, realm context,
 * and the Genesis-parcel lookup.
 *
 * Nothing here is invented: every figure arrives as a `Datum`, and a figure
 * that did not arrive renders as `--` beside the endpoint that failed to
 * produce it.
 */
export default function ChActivityIndexPage({
  address,
  readAt = null,
  peopleInYourWorlds,
  networkPresence,
  worlds,
  busiestScenes,
  busiestWorlds,
  parcel = null,
  parcelPointer = null,
  sources,
  publishHref = "/creator-hub/deploy-world",
  namesHref = "/marketplace/names",
  dataSourcesHref = "/creator-hub/data-sources",
  onRefresh,
  refreshing = false,
  onConnect,
  onAddressSubmit,
  onPointerLookup,
  now,
}: ChActivityIndexPageProps) {
  const addressFieldId = useId();
  const pointerFieldId = useId();
  const [addressDraft, setAddressDraft] = useState("");
  const [pointerDraft, setPointerDraft] = useState(parcelPointer ?? "");

  const stamp = formatReadStamp(readAt, now);
  const tally = tallyStates(
    [peopleInYourWorlds, networkPresence, worlds, busiestScenes, busiestWorlds],
    now,
  );

  const header = (
    <>
      <header className="ai__head">
        <div className="ai__headmain">
          <h1 className="ai__title">Activity</h1>
          <p className="ai__sub">
            Who is in your worlds, sampled every ~5 minutes. Headcount only &#x2014;
            see &#x201C;What we can&#x2019;t tell you&#x201D; below.
          </p>
        </div>
        <div className="ai__headside">
          <button
            type="button"
            className="ai__refresh"
            onClick={onRefresh}
            disabled={refreshing || !onRefresh}
          >
            <span aria-hidden="true">&#x27F3;</span>{" "}
            {refreshing ? "Reading\u{2026}" : "Refresh"}
          </button>
          <p className="ai__stamp">
            {stamp ? `Read at ${stamp}` : "Read time unknown"}
          </p>
        </div>
      </header>
      <DatumTally tally={tally} />
      <p className="ai__disclosure" role="note">
        {PUBLIC_DATA_DISCLOSURE}
      </p>
    </>
  );

  if (address === null) {
    return (
      <div className="ai">
        {header}
        <EmptyState
          className="ai__noaddress"
          variant="screen"
          title={NO_ADDRESS_TITLE}
          subtitle={NO_ADDRESS_BODY}
          actions={
            <div className="ai__noaddressactions">
              <button
                type="button"
                className="ai__connect"
                onClick={onConnect}
                disabled={!onConnect}
              >
                Connect wallet
              </button>
              <form
                className="ai__addressform"
                onSubmit={(event) => {
                  event.preventDefault();
                  const value = addressDraft.trim();
                  if (value) onAddressSubmit?.(value);
                }}
              >
                <label className="ai__addresslabel" htmlFor={addressFieldId}>
                  Or look up any address
                </label>
                <div className="ai__addressrow">
                  <input
                    id={addressFieldId}
                    className="ai__addressinput"
                    name="address"
                    type="text"
                    inputMode="text"
                    autoComplete="off"
                    placeholder={"0x\u{2026}"}
                    value={addressDraft}
                    onChange={(event) => setAddressDraft(event.target.value)}
                  />
                  <button type="submit" className="ai__addresssubmit">
                    Look up
                  </button>
                </div>
                <p className="ai__addresshint">
                  This field is the proof that an address scopes rows rather
                  than protecting them &#x2014; anyone&#x2019;s worlds read the same.
                </p>
              </form>
            </div>
          }
          icon={undefined}
          tone={undefined}
          actionsGap={undefined}
          style={undefined}
        />
        <section className="ai__section" aria-labelledby="ai-ledger-noaddr">
          <h2 className="ai__sectiontitle" id="ai-ledger-noaddr">
            Source ledger
          </h2>
          <SourceLedger groups={sources} now={now} />
        </section>
      </div>
    );
  }

  const worldRows = showable(worlds) ? worlds.value : null;

  return (
    <div className="ai">
      {header}

      <section className="ai__section" aria-labelledby="ai-across">
        <h2 className="ai__sectiontitle" id="ai-across">
          Across your worlds
        </h2>
        <div className="ai__tiles">
          <DatumTile
            label="People in your worlds right now"
            datum={peopleInYourWorlds}
            format={int}
            now={now}
            note={
              showable(peopleInYourWorlds) && peopleInYourWorlds.value === 0
                ? "a real zero \u{2014} the worlds server reports nobody in any of your worlds"
                : undefined
            }
          />
          <DatumTile
            label="Network peers / islands"
            datum={networkPresence}
            now={now}
          />
        </div>
      </section>

      <section className="ai__section" aria-labelledby="ai-worlds">
        <h2 className="ai__sectiontitle" id="ai-worlds">
          Your worlds
          <DatumBadge datum={worlds} now={now} />
        </h2>

        {worldRows === null ? (
          <div className="ai__panel ai__panel--unavailable">
            <p className="ai__panelhead">The world list could not be read.</p>
            <DatumNote datum={worlds} now={now} />
          </div>
        ) : worldRows.length === 0 ? (
          <EmptyState
            className="ai__empty"
            variant="screen"
            title="No worlds are deployed under this address"
            subtitle="worlds-content-server /worlds?authorized_deployer= answered, and returned zero rows for you."
            actions={[
              { label: "Publish to a World", href: publishHref },
              { label: "Get a NAME", href: namesHref, variant: "outline" },
            ]}
            icon={undefined}
            tone={undefined}
            actionsGap={undefined}
            style={undefined}
          />
        ) : (
          <>
            <WorldsTable rows={worldRows} publishHref={publishHref} now={now} />
            <DatumNote datum={worlds} now={now} />
          </>
        )}

        {worldRows !== null && worldRows.length === 0 ? (
          <p className="ai__caveat" role="note">
            NAME minting inside this hub is simulated. The on-chain path and the
            CLI are the ones that actually mint.
          </p>
        ) : null}
      </section>

      <section className="ai__section" aria-labelledby="ai-busiest">
        <h2 className="ai__sectiontitle" id="ai-busiest">
          Busiest right now
        </h2>
        <div className="ai__busiestgrid">
          <BusiestList heading="Scenes" datum={busiestScenes} now={now} />
          <BusiestList heading="Worlds" datum={busiestWorlds} now={now} />
        </div>
      </section>

      <section className="ai__section" aria-labelledby="ai-genesis">
        <h2 className="ai__sectiontitle" id="ai-genesis">
          Genesis City scenes
        </h2>

        <UnbuiltPanel
          title="Your Genesis parcels"
          why="Nothing on this stack maps a wallet to the parcels it deployed to. /presence/current/scenes reports occupancy by pointer with no owner field, and worlds-content-server's authorized_deployer filter covers worlds only."
          today={"a parcel cannot be listed, only looked up \u{2014} the field below reads the same occupancy history for any coordinate."}
        />

        <form
          className="ai__lookup"
          onSubmit={(event) => {
            event.preventDefault();
            const value = pointerDraft.trim();
            if (value) onPointerLookup?.(value);
          }}
        >
          <label className="ai__lookuplabel" htmlFor={pointerFieldId}>
            Look up a parcel by coordinate
          </label>
          <div className="ai__lookuprow">
            <input
              id={pointerFieldId}
              className="ai__lookupinput"
              name="pointer"
              type="text"
              inputMode="text"
              autoComplete="off"
              placeholder="x,y"
              value={pointerDraft}
              onChange={(event) => setPointerDraft(event.target.value)}
            />
            <button type="submit" className="ai__lookupsubmit">
              Look up
            </button>
          </div>
        </form>

        {parcel ? (
          <div className="ai__parcel">
            <h3 className="ai__parceltitle">
              Occupancy at {parcel.pointer}
              {parcel.jumpUrl ? (
                <a
                  className="ai__jump"
                  href={parcel.jumpUrl}
                  target="_blank"
                  rel="noreferrer"
                >
                  Jump in &#x2197;
                </a>
              ) : null}
            </h3>

            {parcel.noHistory ? (
              <EmptyState
                className="ai__empty"
                title="No occupancy recorded for this parcel"
                subtitle={"The presence collector has no snapshots for this pointer \u{2014} that means it has not been in the poll set, not that it had no visitors."}
                actions={
                  parcel.jumpUrl
                    ? [
                        {
                          label: "Open in explorer \u{2197}",
                          href: parcel.jumpUrl,
                          variant: "outline",
                        },
                      ]
                    : undefined
                }
                icon={undefined}
                variant={undefined}
                tone={undefined}
                actionsGap={undefined}
                style={undefined}
              />
            ) : (
              <AnalyticsChart
                series={parcel.series}
                ariaLabel={`Occupancy history for parcel ${parcel.pointer}`}
                gapBands={parcel.gapBands}
              />
            )}

            <div className="ai__tiles">
              <DatumTile
                label="Peak concurrent (sampled)"
                datum={parcel.peak}
                format={int}
                now={now}
              />
              <DatumTile
                label="Snapshots with someone in it"
                datum={parcel.occupied}
                now={now}
              />
              <DatumTile
                label="History begins"
                datum={parcel.historyBegins}
                now={now}
              />
            </div>
          </div>
        ) : null}
      </section>

      <section className="ai__section" aria-labelledby="ai-cant">
        <h2 className="ai__sectiontitle" id="ai-cant">
          What we can&#x2019;t tell you
        </h2>
        <UnbuiltPanel
          title="Who they were, how long they stayed, what device"
          why="Presence persists addresses (scene_occupancy.addresses) but its HTTP API returns counts only. The client half of session and retention analytics exists in this repo with a generated zod model and a drift gate; the server route /creators/me/scenes/stats 404s."
          today={
            <>
              The headcounts above are the whole picture.{" "}
              <a href={dataSourcesHref}>Data sources</a> lists each missing
              endpoint and why.
            </>
          }
        />
      </section>

      <section className="ai__section" aria-labelledby="ai-ledger">
        <h2 className="ai__sectiontitle" id="ai-ledger">
          Source ledger
        </h2>
        <SourceLedger groups={sources} now={now} />
      </section>
    </div>
  );
}
