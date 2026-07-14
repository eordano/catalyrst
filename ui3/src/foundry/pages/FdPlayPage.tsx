import FdGameCard, { type FdGameCardVM } from "../components/FdGameCard";
import FdSection, { FdPageHead } from "../components/FdSection";

export type { FdGameCardVM };

export type FdPlayPageProps = {
  games: readonly FdGameCardVM[];
  /** Set for hosts only — the shelf then offers the registration form. */
  registerHref?: string | null;
  onLinkOpen?: (slug: string, target: "play") => void;
};

export default function FdPlayPage({
  games,
  registerHref = null,
  onLinkOpen,
}: FdPlayPageProps) {
  // A shelf card promises a World to walk into; an undeployed starter cannot
  // keep that promise, so it gets its own rail instead of a playable's spot.
  const playable = games.filter((g) => g.worldName !== null || g.playHref !== null);
  const starters = games.filter((g) => g.worldName === null && g.playHref === null);
  return (
    <div className="fd-page fd-stack">
      <FdPageHead title="The games" />
      <FdSection
        title="On the shelf"
        badge={
          playable.length > 0 ? (
            <span className="fd-chip fd-num">{playable.length}</span>
          ) : undefined
        }
        aside={
          registerHref ? <a href={registerHref}>Register a game</a> : undefined
        }
      >
        {playable.length === 0 ? (
          <p className="fd-empty">No games imported yet.</p>
        ) : (
          <div className="fd-board">
            {playable.map((game) => (
              <FdGameCard key={game.slug} game={game} onLinkOpen={onLinkOpen} />
            ))}
          </div>
        )}
      </FdSection>
      {starters.length > 0 ? (
        <FdSection title="Starting points">
          <div className="fd-board">
            {starters.map((game) => (
              <FdGameCard key={game.slug} game={game} onLinkOpen={onLinkOpen} />
            ))}
          </div>
        </FdSection>
      ) : null}
    </div>
  );
}
