import FoundryBench from "../routes/foundry.console.bench";
import foundry from "@data/fixtures/foundry.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

// One report, from one real run: `python3 -m dclbots.arena --seed 7`. There is
// no run button on this page — runs are operator executions, ingested with
// foundry:ingest-bench, and the page never implies a bot ran because a visitor
// clicked something.
const base = foundry.bench;
const report = base.reports[0];

export default {
  title: "Routes/FoundryBench",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Reports = {
  render: routeStory({
    Component: FoundryBench,
    path: "/foundry/console/bench",
    loaderData: base,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() =>
      expect(canvasElement.textContent).toContain("flagtag-arena"),
    );
  },
};

export const NoRunsYet = {
  render: routeStory({
    Component: FoundryBench,
    path: "/foundry/console/bench",
    loaderData: { ...base, reports: [] },
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() =>
      expect(canvasElement.textContent).toContain("No recorded bot runs"),
    );
  },
};

// A dclbots run ingested from a snapshot with no run.log beside it: the harness
// recorded what it saw, but no verdict was captured, and the row says exactly
// that instead of defaulting to a pass.
export const SnapshotWithoutVerdict = {
  render: routeStory({
    Component: FoundryBench,
    path: "/foundry/console/bench",
    loaderData: {
      ...base,
      reports: [
        {
          ...report,
          id: "br-snapshot-only",
          slug: "flagtag",
          verdict: null,
          checksTotal: null,
          checksFailed: null,
          missingTools: ["get_scene_logs"],
          stubbedTools: [],
          networkWrites: 0,
        },
      ],
    },
  }),
};
