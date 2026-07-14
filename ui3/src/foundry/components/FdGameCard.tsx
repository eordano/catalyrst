import FdVerdictPill from "./FdVerdictPill";
import "./fdgamecard.css";

const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

/** UTC, formatted by hand: the same string has to come out of the server render
 *  and the client hydration, and Intl's output moves with the runtime's ICU. */
export function formatDeployedDate(iso: string | null): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}, ${d.getUTCFullYear()}`;
}

/** Megabytes as the worlds mirror reports them: decimal, not powers of two. */
export function formatSize(bytes: number | null): string | null {
  if (bytes === null || !Number.isFinite(bytes)) return null;
  const mb = bytes / 1_000_000;
  if (mb >= 10) return `${Math.round(mb)} MB`;
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1000))} kB`;
}

export type FdGameCardVM = {
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
  verdict: "pass" | "fail" | null;
  benchRuns: number;
  /** The game page on this site; the card title links here. */
  href: string;
  /** The deep link into the Decentraland client, when a world exists. */
  playHref: string | null;
  editorHref: string;
  gddHref: string | null;
  note: string | null;
};

export type FdGameCardProps = {
  game: FdGameCardVM;
  onLinkOpen?: (slug: string, target: "play" | "editor") => void;
};

export default function FdGameCard({ game, onLinkOpen }: FdGameCardProps) {
  const deployed = formatDeployedDate(game.deployedAt);
  const size = formatSize(game.sizeBytes);
  const asOf = game.importedAt ? formatDeployedDate(game.importedAt) : null;

  return (
    <article className="fd-gamecard">
      <header className="fd-gamecard__head">
        <h3 className="fd-gamecard__title">
          <a href={game.href}>{game.title}</a>
        </h3>
        {game.verdict ? <FdVerdictPill verdict={game.verdict} /> : null}
      </header>

      <p className="fd-gamecard__world fd-mono">{game.worldName ?? "this repository"}</p>

      <dl className="fd-gamecard__facts">
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
          <dt>Bench runs</dt>
          <dd>{game.benchRuns === 0 ? "none yet" : game.benchRuns}</dd>
        </div>
      </dl>

      {game.note ? <p className="fd-gamecard__note">{game.note}</p> : null}

      {deployed && asOf ? (
        <p className="fd-gamecard__asof">
          Worlds facts as of {asOf} — read from the mirror, not re-checked live.
        </p>
      ) : null}

      <p className="fd-gamecard__source">{game.sourceNote}</p>

      <div className="fd-gamecard__links">
        {game.playHref ? (
          <a
            className="fd-gamecard__link"
            href={game.playHref}
            target="_blank"
            rel="noreferrer"
            onClick={() => onLinkOpen?.(game.slug, "play")}
          >
            Play
          </a>
        ) : null}
        <a
          className="fd-gamecard__link"
          href={game.editorHref}
          onClick={() => onLinkOpen?.(game.slug, "editor")}
        >
          Editor
        </a>
        {game.gddHref ? (
          <a className="fd-gamecard__link" href={game.gddHref}>
            Design doc
          </a>
        ) : null}
      </div>
    </article>
  );
}
