import { loader as builderCollectionsLoader } from "../routes/builder.collections";
import { routeStory } from "./lib";

function RedirectSource() {
  return null;
}

const runRealLoader = (args: { request: Request }) =>
  builderCollectionsLoader(args as never);

export default {
  title: "Routes/BuilderCollections",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const RedirectsToCreateWearables = {
  render: routeStory({
    Component: RedirectSource,
    path: "/builder/collections",
    loader: runRealLoader,
  }),
};

export const PreservesQueryString = {
  render: routeStory({
    Component: RedirectSource,
    path: "/builder/collections",
    url: "/builder/collections?utm_source=story&section=hats",
    loader: runRealLoader,
  }),
};
