import type { ReactNode } from "react";
import { dayUTC, groupDigits, plural } from "../fmt";
import Button from "../../atoms/Button";
import FdVerdictPill from "./FdVerdictPill";
import "./fdgamecard.css";
import "./fdcellchip.css";

export const formatDeployedDate = dayUTC;

export type FdMarketCellSlug =
  | "creator-led-social-competition"
  | "community-operated-game-clubs"
  | "collaborative-build-and-play-labs";

/** This program's own reading of the game against the strategy deck's three
 *  gaming cells — a dated judgment, never a fact the deployment entity carries.
 *  `cell` null means "read, and honestly unclassifiable". */
export type FdMarketCellVM = {
  cell: FdMarketCellSlug | null;
  rationale: string;
  confidence: "evidence-backed" | "inferred";
  classifiedAt: string;
  basis: string;
};

/** Display names for the cell slugs — a display mapping, not data. */
export const MARKET_CELL_NAMES: Record<FdMarketCellSlug, string> = {
  "creator-led-social-competition": "Social competition",
  "community-operated-game-clubs": "Game clubs",
  "collaborative-build-and-play-labs": "Build together",
};

export function marketCellName(cell: FdMarketCellSlug | null): string {
  return cell === null ? "unclassified" : MARKET_CELL_NAMES[cell];
}

/** The chip's provenance affordance: who read the game, when, and why. */
export function marketCellTitle(mc: FdMarketCellVM): string {
  return `This program's reading, ${mc.classifiedAt} — ${mc.rationale} (${mc.confidence})`;
}

/** When the deployment facts on this row were read from the worlds mirror. */
export function mirrorProvenance(importedAt: string | null | undefined): string | null {
  const asOf = importedAt ? dayUTC(importedAt) : null;
  return asOf ? `mirror snapshot, read ${asOf}` : null;
}

/** "1,024 parcels" — one spelling of the count wherever it renders. */
export function parcelLabel(parcels: number): string {
  return plural(parcels, "parcel").replace(/^\d+/, groupDigits(parcels));
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
  /** The row's citation, carried in the play loader's payload. The card shows
   *  it through the chips' provenance titles rather than as a sentence. */
  sourceNote: string;
  /** The deployment entity's own scene.json description — the creator's words,
   *  the card's brand line. Template boilerplate is filtered at render. */
  description?: string | null;
  /** The active steward's display label — the person standing behind the game.
   *  Absent/null renders nothing on the card; the claim affordance lives on
   *  the game page's continuity section. */
  stewardName?: string | null;
  /** The most recent recorded scene MOVEMENT after the deploy — a redeploy,
   *  steward note, or stewardship change; never the harness's own bot runs.
   *  Null = nothing moved, and no chip renders rather than repeating the
   *  deploy date. */
  lastMovedAt?: string | null;
  thumbnailUrl?: string | null;
  /** "watch" = the last run's only failures were harness gaps ("cannot
   *  evaluate") — the pill goes neutral and counts what was verified. */
  verdict: "pass" | "fail" | "watch" | null;
  /** Pill text; colour keys off the verdict. Falls back to passed/failed. */
  verdictLabel?: string | null;
  /** Tooltip for the verdict pill: "k of N checks failed on <date>". */
  verdictDetail?: string | null;
  /** Real bot runs — sandbox runs excluded, per the arena-row rule. */
  benchRuns: number;
  /** Sandbox runs on this scene, listed on the runs console, not counted. */
  sandboxRuns?: number;
  /** The game page on this site; the whole card links here. */
  href: string;
  /** The deep link into the Decentraland client, when a world exists. */
  playHref: string | null;
  gddHref: string | null;
  /** This program's market-cell reading, when one exists. Absent/null = the
   *  game has not been read at all, and no chip renders. */
  cell?: FdMarketCellVM | null;
};

export type FdGameCardProps = {
  game: FdGameCardVM;
  onLinkOpen?: (slug: string, target: "play") => void;
};

/** scene.json descriptions the SDK starter ships verbatim: an entity carrying
 *  one has no description of its own, and the page says so instead of quoting
 *  the template as if the creator wrote it (compared trimmed, case-folded). */
const TEMPLATE_DESCRIPTIONS = ["empty scene template"];

export function isTemplateDescription(description: string): boolean {
  return TEMPLATE_DESCRIPTIONS.includes(description.trim().toLowerCase());
}

/** "Circus_Fortune_Teller" → "Circus Fortune Teller" for the heading. */
export function humanizeEntityTitle(title: string): string {
  return title.replace(/_+/g, " ").replace(/\s+/g, " ").trim();
}

/** Two-letter initials of the display title — the monogram tile is a pure
 *  display of the entity's own title column, never invented imagery. */
export function monogram(title: string): string {
  const words = title.trim().split(/\s+/).filter(Boolean);
  const [first, second] = words;
  if (!first) return "";
  if (!second) return first.slice(0, 2).toUpperCase();
  return (first.charAt(0) + second.charAt(0)).toUpperCase();
}

/** A stored fact plus its provenance: the source rides the `title`, and the
 *  sr-only twin carries it where `title` cannot be reached. */
export function FdFactChip({
  prov,
  children,
}: {
  prov: string | null;
  children: ReactNode;
}) {
  return (
    <span className="fd-chip" title={prov ?? undefined}>
      {children}
      {prov ? <span className="u-sr-only"> — {prov}</span> : null}
    </span>
  );
}

export default function FdGameCard({ game, onLinkOpen }: FdGameCardProps) {
  const displayTitle = humanizeEntityTitle(game.title);
  const deployed = dayUTC(game.deployedAt);
  const prov = mirrorProvenance(game.importedAt);
  const size = formatSize(game.sizeBytes);
  const sandboxRuns = game.sandboxRuns ?? 0;
  const hasLinks = Boolean(game.playHref || game.gddHref);

  return (
    <article className="fd-gamecard">
      <a className="fd-gamecard__thumb" href={game.href} tabIndex={-1} aria-hidden="true">
        {game.thumbnailUrl ? (
          <img src={game.thumbnailUrl} alt="" loading="lazy" />
        ) : (
          <span className="fd-gamecard__monogram fd-mono">{monogram(displayTitle)}</span>
        )}
      </a>

      <header className="fd-gamecard__head">
        <h3 className="fd-gamecard__title">
          {/* The whole card is this link's hit area (its ::after stretches over
              the article); the verdict, chips and footer links sit above it. */}
          <a className="fd-gamecard__cardlink" href={game.href}>
            {displayTitle}
          </a>
        </h3>
        {game.verdict ? (
          <a
            className="fd-gamecard__verdictlink"
            href={`${game.href}#bench`}
            title={game.verdictDetail ?? undefined}
            aria-label={
              game.verdictDetail
                ? `${game.verdictLabel ?? (game.verdict === "fail" ? "failed" : "passed")} — ${game.verdictDetail}`
                : game.verdict === "fail"
                  ? "last run failed"
                  : "last run passed"
            }
          >
            <FdVerdictPill
              verdict={game.verdict}
              label={
                game.verdictLabel ??
                (game.verdict === "fail" ? "failed" : "passed")
              }
            />
          </a>
        ) : null}
      </header>

      {game.description && !isTemplateDescription(game.description) ? (
        <p className="fd-gamecard__desc" title={game.sourceNote}>
          {game.description}
        </p>
      ) : null}

      <p className="fd-gamecard__world fd-mono">
        {game.worldName ?? "this repository"}
        {game.stewardName ? (
          <span className="fd-gamecard__steward"> · stewarded by {game.stewardName}</span>
        ) : null}
      </p>

      <p className="fd-gamecard__chips">
        {deployed ? <FdFactChip prov={prov}>Deployed {deployed}</FdFactChip> : null}
        {game.lastMovedAt ? (
          <span
            className="fd-chip"
            title="most recent recorded scene movement after the deploy — a redeploy, steward note, or stewardship change"
          >
            moved {dayUTC(game.lastMovedAt)}
          </span>
        ) : null}
        {size ? <FdFactChip prov={prov}>{size}</FdFactChip> : null}
        {game.parcels !== null ? (
          <FdFactChip prov={prov}>{parcelLabel(game.parcels)}</FdFactChip>
        ) : null}
        {game.benchRuns > 0 ? (
          <a
            className="fd-chip fd-gamecard__chiplink"
            href={`${game.href}#bench`}
            title="stored bench runs, sandbox sims excluded"
          >
            {plural(game.benchRuns, "run")}
          </a>
        ) : sandboxRuns > 0 ? (
          <a
            className="fd-chip fd-gamecard__chiplink"
            href={`${game.href}#bench`}
            title="ran in the sandbox, not against this World"
          >
            {plural(sandboxRuns, "sandbox run")}
          </a>
        ) : null}
        {game.cell ? (
          <span className="fd-cellchip" title={marketCellTitle(game.cell)}>
            {marketCellName(game.cell.cell)} · {dayUTC(game.cell.classifiedAt)}
            <span className="u-sr-only"> — {marketCellTitle(game.cell)}</span>
          </span>
        ) : null}
      </p>

      {hasLinks ? (
        <div className="fd-gamecard__links">
          {game.playHref ? (
            <Button
              as="a"
              variant="primary"
              size="sm"
              href={game.playHref}
              target="_blank"
              rel="noreferrer"
              onClick={() => onLinkOpen?.(game.slug, "play")}
            >
              Play
            </Button>
          ) : null}
          {game.gddHref ? (
            <Button as="a" variant="ghost" size="sm" href={game.gddHref}>
              Design doc
            </Button>
          ) : null}
        </div>
      ) : null}
    </article>
  );
}
