import FoundryHome from "../routes/foundry._index";
import foundry from "@data/fixtures/foundry.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

// The front door is the three doors: navigation, not stats. The loader still
// carries four measured counts, but only for fd_home_viewed's telemetry shape —
// the page renders none of them.
const base = foundry.home;

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
        "An open build bench for Decentraland games",
      );
      // The three doors are the page's primary content.
      expect(canvasElement.textContent).toContain("Show up and play.");
      expect(canvasElement.textContent).toContain(
        "Build something your friends will show up for.",
      );
      expect(canvasElement.textContent).toContain("See how the program runs.");
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
