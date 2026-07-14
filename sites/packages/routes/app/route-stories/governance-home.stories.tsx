import GovernanceHome from "../routes/governance._index";
import homeLoader from "@data/fixtures/route-governance-home.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

const base = homeLoader;

export default {
  title: "Routes/GovernanceHome",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Default = {
  render: routeStory({
    Component: GovernanceHome,
    path: "/governance",
    loaderData: base,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("Add a new Catalyst node");
      expect(canvasElement.textContent).toContain("Funds allocated");
    });
  },
};

export const DataUnavailable = {
  render: routeStory({
    Component: GovernanceHome,
    path: "/governance",
    loaderData: {
      sid: base.sid,
      endingSoon: [],
      fallback: true,
      metrics: [],
      bottomStats: [],
      grants: [],
      topVoters: [],
      chartPoints: [],
      activity: [],
    },
  }),
};
