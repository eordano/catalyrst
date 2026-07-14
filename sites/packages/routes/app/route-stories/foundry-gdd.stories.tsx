import FoundryGddList from "../routes/foundry.gdd";
import FoundryGddDoc from "../routes/foundry.gdd_.$id";
import foundry from "@data/fixtures/foundry.json";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

// Three real shortGDDs written in the Creator Success pre-production format and
// imported from Slack on 2026-08-12. The marker counts below are parsed from
// those documents, not asserted: Pixelwars carries 11 TBD, 10 [HYPOTHESIS] and
// 14 [agent-decided] markers with zero [OPEN] sections; the Alien Human Zoo
// draft is the opposite shape — 9 sections still [OPEN].
const list = foundry.gddList;
const pixelwars = foundry.gddDoc;
const alien = foundry.gddDocs[2];

export default {
  title: "Routes/FoundryGdd",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

export const List = {
  render: routeStory({
    Component: FoundryGddList,
    path: "/foundry/gdd",
    loaderData: list,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() =>
      expect(canvasElement.textContent).toContain("Pixelwars shortGDD"),
    );
  },
};

export const NoDocsYet = {
  render: routeStory({
    Component: FoundryGddList,
    path: "/foundry/gdd",
    loaderData: { ...list, docs: [] },
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() =>
      expect(canvasElement.textContent).toContain("No design docs yet"),
    );
  },
};

// The hypothesis log with all twelve claims parked: the filename state machine
// (H<stage>-<nn>-<slug>_<status>.md) is the only source of those statuses.
export const Document = {
  render: routeStory({
    Component: FoundryGddDoc,
    path: "/foundry/gdd/:id",
    url: "/foundry/gdd/pixelwars-shortgdd-v1",
    loaderData: pixelwars,
  }),
};

// The honest-but-unfinished end of the range: a draft whose open questions are
// still marked [OPEN] section by section, with no hypothesis tree beside it.
export const DocumentWithOpenSections = {
  render: routeStory({
    Component: FoundryGddDoc,
    path: "/foundry/gdd/:id",
    url: "/foundry/gdd/alien-human-zoo-shortgdd-v1",
    loaderData: { ...pixelwars, doc: alien },
  }),
};

// v2 supersedes v1: the review pass is a second version of the same document,
// linked rather than overwritten.
export const DocumentReviewPass = {
  render: routeStory({
    Component: FoundryGddDoc,
    path: "/foundry/gdd/:id",
    url: "/foundry/gdd/pixelwars-shortgdd-v2",
    loaderData: { ...pixelwars, doc: foundry.gddDocs[1] },
  }),
};
