import GovernanceTransparency from "../routes/governance.transparency";
import transparencyFx from "@data/fixtures/route-governance-transparency.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

const base = transparencyFx;

export default {
  title: "Routes/GovernanceTransparency",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Committees = {
  render: routeStory({
    Component: GovernanceTransparency,
    path: "/governance/transparency",
    loaderData: base,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("DAO Committee");
      expect(canvasElement.textContent).toContain("DAO Council");
    });
  },
};

export const Empty = {
  render: routeStory({
    Component: GovernanceTransparency,
    path: "/governance/transparency",
    loaderData: {
      ...base,
      transparency: { source: "empty", committees: [] },
    },
  }),
};
