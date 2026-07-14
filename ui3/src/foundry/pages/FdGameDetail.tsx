import EmptyState from "../../components/EmptyState";
import FdEmbedViewport from "../components/FdEmbedViewport";
import FdSection, { FdPageHead } from "../components/FdSection";
import { formatDeployedDate, formatSize } from "../components/FdGameCard";
import {
  benchStamp,
  FdBenchReportCard,
  type FdBenchReportVM,
  isLocalRealm,
} from "./FdBenchPage";
import "./fdgamedetail.css";

export type FdGameDetailVM = {
  slug: string;
  title: string;
  worldName: string | null;
  deployedAt: string | null;
  /** When these deployment facts were read from the worlds mirror. */
  importedAt?: string | null;
  sizeBytes: number | null;
  parcels: number | null;
  source: "worlds-mirror" | "repo";
  sourceNote: string;
  /** Non-leaking labels (basename + short hash), computed server-side by the
   *  loader from the raw paths so no absolute host path reaches the client. */
  repoLabel: string | null;
  botManifestLabel: string | null;
};

export type FdGameEmbedVM = {
  url: string;
  reachable: boolean;
  probedAt: string;
  /** The exact content URL probed, and the status seen (null on timeout). */
  probedUrl?: string;
  status?: number | null;
};

export type FdGameLinkTarget = "play" | "editor" | "world";

/** The design doc's shape without its body — the game page never loads one. */
export type FdGameGddVM = {
  id: string;
  title: string;
  kind: string;
  version: number;
  open: number;
  hypotheses: number;
};

export type FdGameDetailProps = {
  game: FdGameDetailVM;
  embed: FdGameEmbedVM | null;
  worldHref: string | null;
  editorHref: string;
  gddHref: string | null;
  gdd?: FdGameGddVM | null;
  reports: readonly FdBenchReportVM[];
  /** Total reports for this scene; the list above is capped. */
  reportsTotal?: number;
  embedStarted: boolean;
  onEmbedStart: () => void;
  onLinkOpen: (target: FdGameLinkTarget) => void;
  onReportOpen?: (report: FdBenchReportVM) => void;
};

export default function FdGameDetail({
  game,
  embed,
  worldHref,
  editorHref,
  gddHref,
  gdd = null,
  reports,
  reportsTotal,
  embedStarted,
  onEmbedStart,
  onLinkOpen,
  onReportOpen,
}: FdGameDetailProps) {
  const deployed = formatDeployedDate(game.deployedAt);
  const size = formatSize(game.sizeBytes);
  const asOf = game.importedAt ? formatDeployedDate(game.importedAt) : null;
  const shownReports = reports.length;
  const totalReports = reportsTotal ?? shownReports;

  // Derive the bench-section note from the runs themselves rather than asserting
  // it: only claim "no run has targeted the deployed World" when no real (non-
  // arena) run carried a non-loopback realm, and only claim a "local copy" when a
  // real run actually ran against a loopback realm.
  const benchTargetedWorld = reports.some(
    (r) => r.runner !== "arena" && !!r.realm && !isLocalRealm(r.realm),
  );
  const benchHasLocalCopy = reports.some(
    (r) => r.runner !== "arena" && isLocalRealm(r.realm),
  );
  const benchSub =
    !benchTargetedWorld && benchHasLocalCopy
      ? "Bot runs recorded against a local copy of this scene by the dcl-scene-bots harness — no run to date has targeted the deployed World."
      : "Bot runs recorded against this scene by the dcl-scene-bots harness.";

  return (
    <div className="fd-page fd-stack fd-gamedetail">
      <FdPageHead
        eyebrow={game.worldName ?? "in this repository"}
        title={game.title}
        intro={
          deployed
            ? `Deployed to Decentraland Worlds ${deployed}. Every fact on this page comes from that deployment entity, from this repository, or from a bot run that actually executed.`
            : "This scene lives in this repository and has never been deployed to a World, so it has no deployment date, size or parcel count."
        }
      />

      <dl className="fd-gamedetail__facts">
        <div>
          <dt>Deployed</dt>
          <dd>{deployed ? `to Worlds ${deployed}` : "—"}</dd>
        </div>
        <div>
          <dt>Size</dt>
          <dd>{size ?? "—"}</dd>
        </div>
        <div>
          <dt>Parcels</dt>
          <dd>{game.parcels === null ? "—" : game.parcels}</dd>
        </div>
        <div>
          <dt>World</dt>
          <dd className="fd-mono">{game.worldName ?? "—"}</dd>
        </div>
      </dl>

      {deployed && asOf ? (
        <p className="fd-gamedetail__asof">
          These Worlds facts are a mirror snapshot as of {asOf} — read from the
          worlds mirror at import, not re-checked against the live World on load.
        </p>
      ) : null}

      <FdSection title="Play it">
        {embed === null ? (
          <p className="fd-panel fd-panelnote">
            An SDK7 starter scene living in this repo — open it in the editor.
          </p>
        ) : embed.reachable ? (
          <FdEmbedViewport
            url={embed.url}
            title={`${game.title} in the Decentraland client`}
            weight={size}
            started={embedStarted}
            onStart={onEmbedStart}
          />
        ) : (
          <p className="fd-panel fd-panelnote">
            Probed {embed.probedUrl ?? "the world's content"} at{" "}
            {benchStamp(embed.probedAt)} —{" "}
            {embed.status ? `HTTP ${embed.status}` : "no response"}. The embed is
            withheld until the content is publicly fetchable; the deep link below still
            opens the client.
          </p>
        )}

        <div className="fd-gamedetail__links">
          {worldHref ? (
            <a
              className="fd-gamedetail__link"
              href={worldHref}
              target="_blank"
              rel="noreferrer"
              onClick={() => onLinkOpen("world")}
            >
              Open in the Decentraland client
            </a>
          ) : null}
          <a
            className="fd-gamedetail__link"
            href={editorHref}
            onClick={() => onLinkOpen("editor")}
          >
            Open the editor
          </a>
          {gddHref ? (
            <a className="fd-gamedetail__link" href={gddHref}>
              Design doc
            </a>
          ) : null}
        </div>
      </FdSection>

      {gdd ? (
        <FdSection title="Design doc">
          <div className="fd-panel fd-gamedetail__gdd">
            <p className="fd-gamedetail__gddtitle">
              <a href={gddHref ?? `/foundry/gdd/${gdd.id}`}>{gdd.title}</a>
              <span className="fd-chip">{gdd.kind}</span>
              <span className="fd-chip">v{gdd.version}</span>
            </p>
            <p className="fd-panelnote">
              {gdd.hypotheses === 0
                ? "No hypotheses logged yet"
                : `${gdd.hypotheses} hypothes${gdd.hypotheses === 1 ? "is" : "es"} logged`}
              {" · "}
              {gdd.open === 0
                ? "no section left marked [OPEN]"
                : `${gdd.open} section${gdd.open === 1 ? "" : "s"} still marked [OPEN]`}
              .
            </p>
          </div>
        </FdSection>
      ) : null}

      <FdSection title="Bench runs" sub={benchSub}>
        {reports.length === 0 ? (
          <EmptyState
            variant="inline"
            title="No recorded bot runs for this game."
            subtitle="Runs are executed by the dcl-scene-bots harness and ingested with foundry:ingest-bench."
          />
        ) : (
          <>
            {totalReports > shownReports ? (
              <p className="fd-note">
                Showing the {shownReports} most recent of {totalReports} runs.
              </p>
            ) : null}
            <div className="fd-gamedetail__reports">
              {reports.map((report) => (
                <FdBenchReportCard key={report.id} report={report} onOpen={onReportOpen} />
              ))}
            </div>
          </>
        )}
      </FdSection>

      <FdSection title="Where this row came from">
        <div className="fd-panel">
          <p className="fd-panelnote">{game.sourceNote}</p>
          {game.repoLabel ? (
            <p className="fd-panelnote fd-mono">harness copy: {game.repoLabel}</p>
          ) : null}
          {game.botManifestLabel ? (
            <p className="fd-panelnote fd-mono">
              bot manifest: {game.botManifestLabel}
            </p>
          ) : null}
          {embed ? (
            <p className="fd-panelnote">
              Content reachability was probed at {benchStamp(embed.probedAt)} and came
              back {embed.reachable ? "reachable" : "unreachable"}.
            </p>
          ) : null}
        </div>
      </FdSection>

    </div>
  );
}
