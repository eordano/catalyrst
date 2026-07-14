import { describe, expect, it } from "vitest";

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import FdGddDocPage, { type FdGddDocVM } from "@ui/foundry/pages/FdGddDocPage";
import FdGddListPage, {
  type FdGddListRowVM,
} from "@ui/foundry/pages/FdGddListPage";

// A program-drafted document is unmistakably program-authored everywhere it
// renders: the "recorded" pill (it was produced by an execution that actually
// ran), the "drafted by this program" chip beside it, a repo-path ref that is
// never dressed up as a Slack link, and — for a brief naming no game — the
// open-ground line instead of "proposed".

const totals = { open: 1, tbd: 1, hypothesis: 3, agentDecided: 1 };

const row: FdGddListRowVM = {
  id: "open-ground-game-clubs-v1",
  title: "Open ground: community-operated game clubs",
  kind: "brief",
  version: 1,
  sceneId: null,
  sceneTitle: null,
  source: "program",
  sourceRef: "packages/data/src/fixtures/gdd/open-ground-game-clubs-v1.md",
  honesty: totals,
  hypothesisCounts: { parked: 3 },
  createdAt: "2026-08-17T07:26:41.000Z",
};

const doc: FdGddDocVM = {
  id: row.id,
  title: row.title,
  kind: "brief",
  version: 1,
  sceneId: null,
  supersedes: null,
  source: "program",
  sourceRef: row.sourceRef,
  bodyMd: "## 1. The ground\n\nSized at ([deck slide 09](/foundry/deck#slide-09)).",
  honesty: {
    sections: [{ name: "1. The ground", open: 0, tbd: 0, hypothesis: 0, agentDecided: 0 }],
    totals,
  },
  hypotheses: [],
  createdAt: row.createdAt,
};

describe("a program-drafted row on the design-doc list", () => {
  const html = renderToStaticMarkup(createElement(FdGddListPage, { docs: [row] }));

  it("wears the recorded pill and the program-authorship chip, never a Slack link", () => {
    expect(html).toContain("fd-prov--recorded");
    expect(html).toContain("drafted by this program");
    expect(html).not.toContain("Slack thread");
    // The ref is a repo path shown as text, not an anchor that would 404.
    expect(html).toContain(
      "packages/data/src/fixtures/gdd/open-ground-game-clubs-v1.md",
    );
    expect(html).not.toContain('href="packages/');
  });

  it("says open ground for a brief naming no game — a brief proposes nothing", () => {
    expect(html).toContain("open ground — no game named");
    expect(html).not.toContain("proposed — not built here");
  });
});

describe("a program-drafted brief on its own page", () => {
  const html = renderToStaticMarkup(
    createElement(FdGddDocPage, {
      doc,
      sections: [
        {
          name: "1. The ground",
          contentMd: "Sized at ([deck slide 09](/foundry/deck#slide-09)).",
        },
      ],
      backHref: "/foundry/gdd",
    }),
  );

  it("wears the program-authorship chip and marker totals, never a Slack link or taxonomy chips", () => {
    expect(html).toContain("drafted by this program");
    expect(html).toContain("fd-markers");
    expect(html).not.toContain("Slack thread");
    expect(html).not.toContain("fd-prov--");
    expect(html).not.toContain('fd-chip fd-chip--mono">brief');
  });

  it("carries no shelf disclaimer in either wording", () => {
    expect(html).not.toContain("covers open ground and names no game");
    expect(html).not.toContain("describes a proposed game");
  });

  it("renders the section's citations as anchors, and the document only once", () => {
    expect(html).toContain('<a href="/foundry/deck#slide-09">deck slide 09</a>');
    // The document is rendered, not also dumped as its own markdown source.
    expect(html).not.toContain("[deck slide 09](/foundry/deck#slide-09)");
  });
});
