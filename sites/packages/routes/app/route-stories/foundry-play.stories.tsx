import FoundryPlay from "../routes/foundry.play";
import FoundryGame from "../routes/foundry.play_.$slug";
import foundry from "@data/fixtures/foundry.json";
import emotionalJobs from "@data/fixtures/foundry-emotional-jobs.json";
import marketCells from "@data/fixtures/foundry-market-cells.json";
import type { FdEmotionalJobVM } from "@ui/foundry/components/FdEmotionalJobs";
import type { FdMarketCellVM } from "@ui/foundry/components/FdGameCard";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

// Eight rows: the seven creator games as the worlds mirror really holds them
// (titles, entity ids, deployment dates, sizes, parcel counts) plus the SDK7
// template that lives in this repository. The market-cell readings come from
// the same fixture the importer lands — the judgment text is never typed here.
const cellOf = (slug: string): FdMarketCellVM | null => {
  const row = marketCells.find((r) => r.sceneId === slug);
  return row
    ? ({
        cell: row.cell,
        rationale: row.rationale,
        confidence: row.confidence,
        classifiedAt: row.classifiedAt,
        basis: row.basis,
      } as FdMarketCellVM)
    : null;
};

// The emotional-job reads come from the same fixture the importer lands — the
// judgment text is never typed here.
const jobsOf = (slug: string): FdEmotionalJobVM[] =>
  emotionalJobs
    .filter((r) => r.sceneId === slug)
    .map(
      (r) =>
        ({
          job: r.job,
          rationale: r.rationale,
          confidence: r.confidence,
          readAt: r.readAt,
          basis: r.basis,
        }) as FdEmotionalJobVM,
    );

const base = {
  ...foundry.play,
  games: foundry.play.games.map((g) => ({ ...g, cell: cellOf(g.slug) })),
};
const game = {
  ...foundry.game,
  marketCell: cellOf(foundry.game.slug),
  emotionalJobs: jobsOf(foundry.game.slug),
};

export default {
  title: "Routes/FoundryPlay",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Games = {
  render: routeStory({
    Component: FoundryPlay,
    path: "/foundry/play",
    loaderData: base,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("Flag Tag");
      expect(canvasElement.textContent).toContain("Template game");
      // The shelf groups by this program's market-cell reading, and the empty
      // game-clubs cell is shown as the finding it is.
      expect(canvasElement.textContent).toContain("Social competition");
      expect(canvasElement.textContent).toContain("Nothing here yet.");
      // The chip carries the reading's provenance in its title.
      const chip = canvasElement.querySelector(".fd-cellchip");
      expect(chip?.getAttribute("title")).toContain("This program's reading");
    });
  },
};

// Before foundry:import-market-cells has run: no classification rows, so the
// shelf keeps its single ungrouped section — no phantom grouping.
export const GamesNotYetClassified = {
  render: routeStory({
    Component: FoundryPlay,
    path: "/foundry/play",
    loaderData: {
      ...base,
      games: foundry.play.games.map((g) => ({ ...g, cell: null })),
    },
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("On the bench");
      expect(canvasElement.textContent).not.toContain("Game clubs");
    });
  },
};

// Before `foundry:import-real` has ever run. Nothing is invented to fill the
// grid — the page says where the rows come from instead.
export const NothingImportedYet = {
  render: routeStory({
    Component: FoundryPlay,
    path: "/foundry/play",
    loaderData: { ...base, games: [] },
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() =>
      expect(canvasElement.textContent).toContain("No games imported yet"),
    );
  },
};

// The embed is gated on a real HEAD probe of the world's content. Until the
// worlds.example.com reverse-proxy entry lands that probe honestly fails, so the
// default game page shows the deep link and the reason, not a dead iframe.
export const GameNotPubliclyReachable = {
  render: routeStory({
    Component: FoundryGame,
    path: "/foundry/play/:slug",
    url: "/foundry/play/flagtag",
    loaderData: game,
  }),
};

export const GameReachable = {
  render: routeStory({
    Component: FoundryGame,
    path: "/foundry/play/:slug",
    url: "/foundry/play/flagtag",
    loaderData: { ...game, embed: { ...game.embed, reachable: true } },
  }),
};

// Before foundry:import-emotional-jobs has run: no job rows, so the section
// says so instead of inventing a reading.
export const GameNotYetJobRead = {
  render: routeStory({
    Component: FoundryGame,
    path: "/foundry/play/:slug",
    url: "/foundry/play/flagtag",
    loaderData: { ...game, emotionalJobs: [] },
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain(
        "Not yet read against the deck’s six emotional jobs.",
      );
    });
  },
};

// A game with no ingested bot run yet: the verdict chip is absent rather than
// defaulted to a pass.
export const GameWithoutBenchRun = {
  render: routeStory({
    Component: FoundryGame,
    path: "/foundry/play/:slug",
    url: "/foundry/play/flagtag",
    loaderData: { ...game, reports: [] },
  }),
};
