import FoundryPlay from "../routes/foundry.play";
import FoundryGame from "../routes/foundry.play_.$slug";
import foundry from "@data/fixtures/foundry.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

// Eight rows: the seven creator games as the worlds mirror really holds them
// (titles, entity ids, deployment dates, sizes, parcel counts) plus the SDK7
// template that lives in this repository.
const base = foundry.play;
const game = foundry.game;

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
