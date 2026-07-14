import FoundrySelect from "../routes/foundry.select";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

const loaderData = { badge: "visitor-7f3a" };

export default {
  title: "Routes/FoundrySelect",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

// Signed out: Decentraland base avatars and the next session. The doors live
// on /foundry alone now, linked from the head.
export const Default = {
  render: routeStory({
    Component: FoundrySelect,
    path: "/foundry/select",
    loaderData,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("Your people");
      expect(canvasElement.textContent).toContain("Avatars");
      expect(canvasElement.textContent).toContain("Next session");
      expect(canvasElement.textContent).toContain("The three doors");
    });
  },
};
