import EmptyState from "../../components/EmptyState";
import FdGameCard, { type FdGameCardVM } from "../components/FdGameCard";
import FdSection, { FdPageHead } from "../components/FdSection";
import "./fdplay.css";

export type { FdGameCardVM };

export type FdPlayPageProps = {
  games: readonly FdGameCardVM[];
  onLinkOpen?: (slug: string, target: "play" | "editor") => void;
};

export default function FdPlayPage({ games, onLinkOpen }: FdPlayPageProps) {
  // Derived from the rows, not typed in: the count follows the registry.
  const deployedCount = games.filter((g) => g.source === "worlds-mirror").length;
  const hasTemplate = games.some((g) => g.source === "repo");
  const intro =
    deployedCount > 0
      ? `${deployedCount} game${deployedCount === 1 ? "" : "s"} deployed to Decentraland Worlds${
          hasTemplate ? ", plus the SDK7 template" : ""
        }. Everything on this page is read from the Worlds mirror and this repository — the dates are the deployment entities' own.`
      : "Everything on this page is read from the Worlds mirror and this repository — the dates are the deployment entities' own.";

  return (
    <div className="fd-page fd-stack fd-play">
      <FdPageHead title="The games" intro={intro} />

      <FdSection title="On the bench">
        {games.length === 0 ? (
          <EmptyState
            variant="inline"
            title="No games imported yet."
            subtitle="The registry fills from the real Worlds deployments via foundry:import-real — nothing is invented to fill the gap."
          />
        ) : (
          <div className="fd-play__grid">
            {games.map((game) => (
              <FdGameCard key={game.slug} game={game} onLinkOpen={onLinkOpen} />
            ))}
          </div>
        )}
      </FdSection>

    </div>
  );
}
