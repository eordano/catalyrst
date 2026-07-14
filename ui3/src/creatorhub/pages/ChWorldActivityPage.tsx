import type { ComponentType, ReactNode } from "react";
import EmptyState from "../../components/EmptyState";
import AnalyticsChart from "../components/AnalyticsChart";
import CliEscape, { type CliEscapeProps } from "../components/CliEscape";
import CreatorHubBreadcrumb from "../components/CreatorHubBreadcrumb";
import DatumBadge, { DatumTally } from "../components/DatumBadge";
import DatumNote from "../components/DatumNote";
import DatumTile from "../components/DatumTile";
import SourceLedger, {
  type SourceLedgerGroup,
} from "../components/SourceLedger";
import UnbuiltPanel from "../components/UnbuiltPanel";
import {
  NO_VALUE,
  PUBLIC_DATA_DISCLOSURE,
  disagree,
  formatReadStamp,
  formatUtcDay,
  showable,
  tallyStates,
  type Datum,
} from "../lib/datum";
import type { ChartSeries } from "../lib/scene-analytics";
import "./chworldactivity.css";

export type WorldMeta = {
  title: string | null;
  owner: string | null;
  lastDeployedAt: string | null;
  deployedScenes: number | null;
  blockedSince: string | null;
};

export type WorldHistory = {
  series: ChartSeries[];
  gapBands?: { fromIndex: number; toIndex: number }[];
};

export type StorageReading = {
  /** Preformatted by the data layer -- the byte strings are BigInt, not Number. */
  label: string;
  /** 0...1, or `null` when the quota is unknown. Drives the bar only. */
  ratio: number | null;
};

export type FactRow = { label: string; value: string };

export type NotBuiltSpec = {
  id: string;
  title: string;
  why: string;
  today?: ReactNode;
  todayCli?: CliEscapeProps;
};

type LinkComponentProps = {
  className?: string;
  to: string;
  prefetch?: "intent" | "render" | "none" | "viewport";
  children?: ReactNode;
};

export type ChWorldActivityPageProps = {
  world: string;
  worldMeta: Datum<WorldMeta>;
  jumpUrl?: string | null;
  readAt?: string | null;

  /** Right now */
  inThisWorld: Datum<number>;
  commsRoom: Datum<number>;
  realm: Datum<string>;

  /** Who was here */
  history: Datum<WorldHistory>;
  peak: Datum<number>;
  occupiedSnapshots: Datum<string>;
  historyBegins: Datum<string>;
  onRetryHistory?: () => void;

  /** What is deployed */
  sceneUrn: Datum<string>;
  spawnCoordinates: Datum<string>;
  storage: Datum<StorageReading>;
  /**
   * The scene key-value store. This panel has no value slot at all: the
   * endpoint needs an ADR-44 signed fetch made by the scene runtime, and it
   * would be a different number from deployed bytes even if it answered.
   */
  sceneKvStorage: Datum<unknown>;

  /** Who can get in -- read-only, always */
  access: Datum<readonly FactRow[]>;
  permissionsCli?: CliEscapeProps;
  permissionsHref?: string;

  /** Reception */
  reception: Datum<readonly FactRow[]>;

  notBuilt: readonly NotBuiltSpec[];
  sources: readonly SourceLedgerGroup[];

  /** Full-page states */
  notFound?: boolean;
  /** False renders the neutral "not yours, still public" line. Never a lock. */
  deployedByCaller?: boolean;

  backTo?: string;
  backLabel?: string;
  LinkComponent?: ComponentType<LinkComponentProps>;
  worldsHref?: string;
  onRefresh?: () => void;
  refreshing?: boolean;
  /** Pinned clock, for deterministic stories and tests. */
  now?: number;
};

const int = (v: number | string) =>
  typeof v === "number" ? v.toLocaleString("en-US") : v;

function isEmptyHistory(history: WorldHistory): boolean {
  return (
    history.series.length === 0 ||
    history.series.every((s) => s.points.length === 0)
  );
}

function FactList({
  datum,
  emptyNote,
  now,
}: {
  datum: Datum<readonly FactRow[]>;
  emptyNote: string;
  now?: number;
}) {
  return (
    <>
      {showable(datum) ? (
        datum.value.length === 0 ? (
          <p className="wa__factsempty">{emptyNote}</p>
        ) : (
          <dl className="wa__facts">
            {datum.value.map((row) => (
              <div className="wa__fact" key={row.label}>
                <dt>{row.label}</dt>
                <dd>{row.value}</dd>
              </div>
            ))}
          </dl>
        )
      ) : null}
      <DatumNote datum={datum} now={now} />
    </>
  );
}

/**
 * `/creator-hub/activity/:world` -- one world, on one scrolling page.
 *
 * The gaps are deliberately adjacent to the data rather than filed behind a
 * tab: what this page cannot tell you is as load-bearing as what it can.
 * Degradation is partial by design -- a dead presence collector empties the
 * right-now and history sections and leaves deploy, access and reception alone.
 */
export default function ChWorldActivityPage({
  world,
  worldMeta,
  jumpUrl = null,
  readAt = null,
  inThisWorld,
  commsRoom,
  realm,
  history,
  peak,
  occupiedSnapshots,
  historyBegins,
  onRetryHistory,
  sceneUrn,
  spawnCoordinates,
  storage,
  sceneKvStorage,
  access,
  permissionsCli,
  permissionsHref,
  reception,
  notBuilt,
  sources,
  notFound = false,
  deployedByCaller = true,
  backTo = "/creator-hub/activity",
  backLabel = "Back to Activity",
  LinkComponent,
  worldsHref = "/creator-hub/activity",
  onRefresh,
  refreshing = false,
  now,
}: ChWorldActivityPageProps) {
  if (notFound) {
    return (
      <div className="wa">
        <CreatorHubBreadcrumb
          to={backTo}
          label={backLabel}
          LinkComponent={LinkComponent}
        />
        <EmptyState
          variant="screen"
          tone="error"
          title={`${world} was not found`}
          subtitle={`${world} is not on worlds-content-server and the catalyst has no /world/${world}/about for it.`}
          actions={[{ label: "See your worlds", href: worldsHref }]}
          icon={undefined}
          actionsGap={undefined}
          style={undefined}
        />
      </div>
    );
  }

  const meta = showable(worldMeta) ? worldMeta.value : null;
  const stamp = formatReadStamp(readAt, now);
  const tally = tallyStates(
    [
      inThisWorld,
      commsRoom,
      realm,
      history,
      sceneUrn,
      spawnCoordinates,
      storage,
      sceneKvStorage,
      access,
      reception,
    ],
    now,
  );

  const showDisagreement = disagree(inThisWorld, commsRoom);
  const presenceValue = showable(inThisWorld) ? inThisWorld.value : null;
  const commsValue = showable(commsRoom) ? commsRoom.value : null;

  return (
    <div className="wa">
      <CreatorHubBreadcrumb
        to={backTo}
        label={backLabel}
        LinkComponent={LinkComponent}
      />

      <header className="wa__head">
        <div className="wa__headmain">
          <h1 className="wa__title">
            {world}
            {meta?.title ? (
              <span className="wa__worldtitle">&#x201C;{meta.title}&#x201D;</span>
            ) : null}
          </h1>
          {meta ? (
            <p className="wa__meta">
              {meta.owner ? <span>owner {meta.owner}</span> : null}
              {meta.deployedScenes !== null ? (
                <span>
                  {meta.deployedScenes}{" "}
                  {meta.deployedScenes === 1 ? "scene" : "scenes"}
                </span>
              ) : null}
              {meta.lastDeployedAt ? (
                <span>
                  last deployed {formatUtcDay(meta.lastDeployedAt) ?? NO_VALUE}
                </span>
              ) : null}
              {meta.blockedSince ? (
                <span className="wa__blocked">
                  blocked since {formatUtcDay(meta.blockedSince)}
                </span>
              ) : null}
            </p>
          ) : (
            <DatumNote datum={worldMeta} now={now} />
          )}
        </div>

        <div className="wa__headside">
          {jumpUrl ? (
            <a
              className="wa__jump"
              href={jumpUrl}
              target="_blank"
              rel="noreferrer"
            >
              Jump in &#x2197;
            </a>
          ) : null}
          <button
            type="button"
            className="wa__refresh"
            onClick={onRefresh}
            disabled={refreshing || !onRefresh}
          >
            <span aria-hidden="true">&#x27F3;</span>{" "}
            {refreshing ? "Reading\u{2026}" : "Refresh"}
          </button>
          <p className="wa__stamp">
            {stamp ? `Read at ${stamp}` : "Read time unknown"}
          </p>
        </div>
      </header>

      <DatumTally tally={tally} />

      <p className="wa__disclosure" role="note">
        {PUBLIC_DATA_DISCLOSURE}
      </p>

      {deployedByCaller ? null : (
        <p className="wa__notyours" role="note">
          This world isn&#x2019;t deployed by your address. Occupancy is public &#x2014;
          anyone can read these counts for any world.
        </p>
      )}

      <section className="wa__section" aria-labelledby="wa-now">
        <h2 className="wa__sectiontitle" id="wa-now">
          Right now
        </h2>
        <div className="wa__tiles">
          <DatumTile
            label="In this world"
            datum={inThisWorld}
            format={int}
            now={now}
            note={
              showable(inThisWorld) && inThisWorld.value === 0
                ? "a real zero \u{2014} nobody in it at the last snapshot"
                : undefined
            }
          />
          <DatumTile
            label="Comms room"
            datum={commsRoom}
            format={int}
            now={now}
            note={
              showable(commsRoom) && commsRoom.value === 0
                ? "a real zero \u{2014} the worlds server reports an empty room"
                : undefined
            }
          />
          <DatumTile label="Realm" datum={realm} now={now} />
        </div>

        {showDisagreement ? (
          <p className="wa__disagree" role="note">
            These disagree ({presenceValue} vs {commsValue}) and both are right:
            presence samples every 5 minutes and counts distinct addresses in
            comms; <code>/live-data</code> is instant and is the worlds server&#x2019;s
            own figure. Neither is &#x201C;users online&#x201D;.
          </p>
        ) : null}
      </section>

      <section className="wa__section" aria-labelledby="wa-history">
        <h2 className="wa__sectiontitle" id="wa-history">
          Who was here
          <DatumBadge datum={history} now={now} />
        </h2>

        {!showable(history) ? (
          <EmptyState
            className="wa__charterror"
            tone="error"
            title="Occupancy history could not be read"
            subtitle={
              history.state === "unbuilt"
                ? history.reason
                : `${history.endpoint} did not return a usable series.`
            }
            actions={
              onRetryHistory
                ? [{ label: "\u{27F3} Try again", onClick: onRetryHistory }]
                : undefined
            }
            icon={undefined}
            variant={undefined}
            actionsGap={undefined}
            style={undefined}
          />
        ) : isEmptyHistory(history.value) ? (
          <EmptyState
            className="wa__chartempty"
            title="No occupancy recorded for this world"
            subtitle={`The presence collector has no snapshots for ${world} \u{2014} that means it has not been in the poll set, not that it had no visitors.`}
            actions={
              jumpUrl
                ? [
                    {
                      label: "Open in explorer \u{2197}",
                      href: jumpUrl,
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
            series={history.value.series}
            ariaLabel={`Occupancy history for ${world}`}
            gapBands={history.value.gapBands}
          />
        )}

        <DatumNote datum={history} now={now} />

        <div className="wa__tiles">
          <DatumTile
            label="Peak concurrent (sampled)"
            datum={peak}
            format={int}
            now={now}
          />
          <DatumTile
            label="Snapshots with someone in it"
            datum={occupiedSnapshots}
            now={now}
          />
          <DatumTile label="History begins" datum={historyBegins} now={now} />
        </div>
      </section>

      <section className="wa__section" aria-labelledby="wa-deployed">
        <h2 className="wa__sectiontitle" id="wa-deployed">
          What is deployed
        </h2>
        <div className="wa__tiles">
          <DatumTile label="Scene URN" datum={sceneUrn} now={now} />
          <DatumTile
            label="Spawn coordinates"
            datum={spawnCoordinates}
            now={now}
          />
        </div>

        <div
          className={
            showable(storage)
              ? "wa__storage"
              : "wa__storage wa__storage--unavailable"
          }
        >
          <div className="wa__storagehead">
            <span className="wa__storagelabel">Deployed bytes for this NAME</span>
            <DatumBadge datum={storage} now={now} />
          </div>
          <p className="wa__storagevalue">
            <span
              className={
                showable(storage)
                  ? "wa__storagenum"
                  : "wa__storagenum wa__storagenum--absent"
              }
            >
              {showable(storage) ? storage.value.label : NO_VALUE}
            </span>
          </p>
          {showable(storage) && storage.value.ratio !== null ? (
            <div
              className="wa__bar"
              role="img"
              aria-label={`Quota used: ${storage.value.label}`}
            >
              <span
                className="wa__barfill"
                style={{
                  inlineSize: `${Math.min(100, Math.max(0, storage.value.ratio * 100))}%`,
                }}
              />
            </div>
          ) : null}
          <p className="wa__storagecaption">
            Deployed-content bytes on worlds-content-server. Worlds deployed to
            catalyst.example.com are not counted here.
          </p>
          <DatumNote datum={storage} now={now} />
        </div>

        {/* No value slot on purpose -- this panel can only ever explain itself. */}
        <div className="wa__kv">
          <div className="wa__storagehead">
            <span className="wa__storagelabel">Scene key&#x2013;value storage</span>
            <DatumBadge datum={sceneKvStorage} now={now} />
          </div>
          <p className="wa__kvwhy">
            This is gated <em>and</em> it is a different number. The endpoint
            needs an ADR-44 signed fetch made by the scene runtime (realm +
            parcel metadata); this hub holds no such identity. It sums the
            key&#x2013;value store your scene writes at runtime, not the bytes you
            deployed.
          </p>
          <DatumNote datum={sceneKvStorage} now={now} />
        </div>
      </section>

      <section className="wa__section" aria-labelledby="wa-access">
        <h2 className="wa__sectiontitle" id="wa-access">
          Who can get in
          <DatumBadge datum={access} now={now} />
        </h2>
        <div className="wa__panel">
          <FactList
            datum={access}
            emptyNote="The permissions document is empty: no deployment allow-list, no streaming grant, unrestricted access."
            now={now}
          />
        </div>

        <div className="wa__changing">
          <p className="wa__changingintro">
            This view is read-only. Changing it happens elsewhere:
          </p>
          {permissionsCli ? <CliEscape {...permissionsCli} /> : null}
          {permissionsHref ? (
            <p className="wa__changinglink">
              <a href={permissionsHref}>World permissions</a> owns that flow.
            </p>
          ) : null}
        </div>
      </section>

      <section className="wa__section" aria-labelledby="wa-reception">
        <h2 className="wa__sectiontitle" id="wa-reception">
          Reception
          <DatumBadge datum={reception} now={now} />
        </h2>
        <div className="wa__panel">
          <FactList
            datum={reception}
            emptyNote="The Places API has no record for this world yet."
            now={now}
          />
        </div>
        <p className="wa__excluded" role="note">
          That response also carries <code>user_visits</code> and{" "}
          <code>user_count</code>. Both read 0 for every world we sampled, so
          they are not shown.
        </p>
      </section>

      {notBuilt.length > 0 ? (
        <section className="wa__section" aria-labelledby="wa-notbuilt">
          <h2 className="wa__sectiontitle" id="wa-notbuilt">
            Not built yet
          </h2>
          <div className="wa__notbuiltgrid">
            {notBuilt.map((panel) => (
              <UnbuiltPanel
                key={panel.id}
                title={panel.title}
                why={panel.why}
                today={
                  panel.todayCli ? (
                    <CliEscape {...panel.todayCli} />
                  ) : (
                    panel.today
                  )
                }
              />
            ))}
          </div>
        </section>
      ) : null}

      <section className="wa__section" aria-labelledby="wa-ledger">
        <h2 className="wa__sectiontitle" id="wa-ledger">
          Source ledger
        </h2>
        <SourceLedger groups={sources} now={now} />
      </section>
    </div>
  );
}
