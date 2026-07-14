import FoundryTrajectories from "../routes/foundry.console.trajectories";
import FoundryReplay from "../routes/foundry.console.trajectories_.$id";
import foundry from "@data/fixtures/foundry.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

// The episode is the arena run's own stdout, one obs/snapshot event per printed
// line, bracketed by the turn it belongs to. The harness prints no per-line
// timestamps, so every event carries the run's time — the replay shows no
// duration it cannot derive.
const list = foundry.trajectories;
const replay = foundry.replay;

export default {
  title: "Routes/FoundryReplay",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Episodes = {
  render: routeStory({
    Component: FoundryTrajectories,
    path: "/foundry/console/trajectories",
    loaderData: list,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() =>
      expect(canvasElement.textContent).toContain("Flag Tag"),
    );
  },
};

export const NoEpisodesYet = {
  render: routeStory({
    Component: FoundryTrajectories,
    path: "/foundry/console/trajectories",
    loaderData: { ...list, records: [] },
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() =>
      expect(canvasElement.textContent).toContain("No episodes recorded"),
    );
  },
};

export const Replay = {
  render: routeStory({
    Component: FoundryReplay,
    path: "/foundry/console/trajectories/:id",
    url: "/foundry/console/trajectories/tr-arena-seed7",
    loaderData: replay,
  }),
};

// An episode still inside its opening turn: the ledger stops where the log
// stops, and the unclosed bracket gets no invented end time.
export const ReplayOpenTurn = {
  render: routeStory({
    Component: FoundryReplay,
    path: "/foundry/console/trajectories/:id",
    url: "/foundry/console/trajectories/tr-arena-seed7",
    loaderData: {
      ...replay,
      header: { ...replay.header, finishReason: null },
      events: replay.events.slice(0, 4),
      eventCount: 4,
    },
  }),
};
