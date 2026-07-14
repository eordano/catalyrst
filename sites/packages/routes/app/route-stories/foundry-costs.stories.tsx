import FoundryCosts from "../routes/foundry.console.costs";
import foundry from "@data/fixtures/foundry.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

// Token counts come from the gateway's own per-message accounting; the dollar
// column is reference-priced at a chosen constant and says so in the footer.
const base = foundry.costs;

export default {
  title: "Routes/FoundryCosts",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Ledger = {
  render: routeStory({
    Component: FoundryCosts,
    path: "/foundry/console/costs",
    loaderData: base,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() =>
      expect(canvasElement.textContent).toContain("reference pricing"),
    );
  },
};

export const NoUsageRecordedYet = {
  render: routeStory({
    Component: FoundryCosts,
    path: "/foundry/console/costs",
    loaderData: {
      ...base,
      usage: {
        messages: 0,
        sessions: 0,
        inputTokens: 0,
        outputTokens: 0,
        costUsd: 0,
        byDay: [],
        recent: [],
      },
    },
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() =>
      expect(canvasElement.textContent).toContain(
        "No copilot usage recorded yet",
      ),
    );
  },
};
