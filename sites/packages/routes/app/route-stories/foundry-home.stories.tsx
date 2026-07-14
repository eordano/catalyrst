import FoundryHome from "../routes/foundry._index";
import foundry from "@data/fixtures/foundry.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

// Real counts: eight registered games, three imported design docs, one recorded
// bench run and the tokens one measured copilot exchange really spent.
const base = foundry.home;
const empty = {
  ...base,
  stats: {
    scenes: 0,
    gddDocs: 0,
    benchRuns: 0,
    lastBenchAt: null,
    copilotOnline: false,
    tokens: 0,
  },
  checks: base.checks.map((c) => ({ ...c, value: "—" })),
};

export default {
  title: "Routes/FoundryHome",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Default = {
  render: routeStory({
    Component: FoundryHome,
    path: "/foundry",
    loaderData: base,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("The Foundry");
      expect(canvasElement.textContent).toContain(
        "Real games, real tests, real costs.",
      );
    });
  },
};

// Both arms render the same page; the auto-tour arm additionally navigates to
// ?tour=1 on the first front-door view, and the tour card itself belongs to the
// foundry.tsx layout above this route.
export const AutoTourArm = {
  render: routeStory({
    Component: FoundryHome,
    path: "/foundry",
    url: "/foundry?tour=1&tour_src=auto",
    loaderData: { ...base, variant: "auto-tour", tourAutoStart: true },
  }),
};

// A migrated database with nothing imported yet: every check keeps its source
// sentence and shows an em dash instead of a number. This is the state the
// empty-DB honesty e2e asserts against.
export const NothingImportedYet = {
  render: routeStory({
    Component: FoundryHome,
    path: "/foundry",
    loaderData: empty,
  }),
};

// The copilot probe is a live server-side HTTP call; when the service is down
// the front door says so rather than dropping the card.
export const CopilotOffline = {
  render: routeStory({
    Component: FoundryHome,
    path: "/foundry",
    loaderData: { ...base, stats: { ...base.stats, copilotOnline: false } },
  }),
};
