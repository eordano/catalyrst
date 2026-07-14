import DiscoverRoute from "../routes/discover";
import home from "@data/fixtures/route-discover-home.json";
import { routeStory } from "./lib";

const base = home;

export default {
  title: "Routes/Discover",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const Live = {
  render: routeStory({
    Component: DiscoverRoute,
    path: "/discover",
    loaderData: base,
  }),
};

export const Offline = {
  render: routeStory({
    Component: DiscoverRoute,
    path: "/discover",
    loaderData: {
      ...base,
      live: false,
      content: {
        ...base.content,
        hero: { ...base.content.hero, downloads: null },
        comeHangOut: { ...base.content.comeHangOut, downloads: null },
        rails: base.content.rails.map((r) => ({ ...r, items: [] })),
      },
    },
  }),
};
