import FoundrySelect from "../routes/foundry.select";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

const loaderData = { badge: "visitor-7f3a" };

export default {
  title: "Routes/FoundrySelect",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

// Signed out: five Decentraland base avatars, no friend count, three doors.
export const Default = {
  render: routeStory({
    Component: FoundrySelect,
    path: "/foundry/select",
    loaderData,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("Three doors. One society.");
      expect(canvasElement.textContent).toContain("Start playing");
      expect(canvasElement.textContent).toContain("Start building");
      expect(canvasElement.textContent).toContain("Start hosting");
    });
  },
};
