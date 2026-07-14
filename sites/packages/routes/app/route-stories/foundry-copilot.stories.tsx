import FoundryCopilot from "../routes/foundry.copilot";
import foundry from "@data/fixtures/foundry.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

// The usage strip is one really measured exchange against llm.decent.dev:
// 9623 input tokens, 76 output, cost 0.0009851 as the gateway reported it. The
// dollar figure carries the reference-pricing label wherever it is shown.
const base = foundry.copilot;

export default {
  title: "Routes/FoundryCopilot",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Online = {
  render: routeStory({
    Component: FoundryCopilot,
    path: "/foundry/copilot",
    loaderData: base,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() =>
      expect(canvasElement.textContent).toContain("Open the copilot"),
    );
  },
};

// The status pill is a live server-side probe of the service over loopback.
// Offline is a fact about this server, stated as one — the skills catalog and
// the pipeline description stay, because they are still true.
export const Offline = {
  render: routeStory({
    Component: FoundryCopilot,
    path: "/foundry/copilot",
    loaderData: {
      ...base,
      status: { online: false, probedAt: base.status.probedAt },
    },
  }),
};

export const NoUsageRecordedYet = {
  render: routeStory({
    Component: FoundryCopilot,
    path: "/foundry/copilot",
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
};
