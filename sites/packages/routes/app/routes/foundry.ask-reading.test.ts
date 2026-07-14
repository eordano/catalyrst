import { describe, expect, it } from "vitest";

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import FdRequestCard, {
  type FdRequestCardVM,
} from "@ui/foundry/components/FdRequestCard";
import FdAskPage, { type FdAskReadingVM } from "@ui/foundry/pages/FdAskPage";

// The reading chip on the board links into the full reading on the ask's page,
// and the ask page's demand lines cite the deck by link — every judgment one
// click from its source, nothing self-praising around the pledge count.

const noop = () => undefined;

const ask: FdRequestCardVM = {
  id: "ask-924f331f1bede6dc",
  title: "A virtual gym you connect your home fitness machine to",
  body: "I would use a virtual gym with my home fitness machine.",
  source: "r/decentraland",
  status: "open",
  pledges: 0,
  pledgedByMe: false,
  origin: "imported",
  reading: { cell: "community-operated-game-clubs", readAt: "2026-08-17" },
};

const reading: FdAskReadingVM = {
  cell: "community-operated-game-clubs",
  jobs: ["F"],
  shelfAnswer: null,
  rationale: "A recurring venue framing, routine attendance implied.",
  confidence: "inferred",
  readAt: "2026-08-17",
  basis: "This program's own reading — never a fact the ask carries.",
  crowdRange: "8–50 recurring participants",
};

function renderCard(vm: FdRequestCardVM): string {
  return renderToStaticMarkup(
    createElement(FdRequestCard, { ...vm, onPledge: noop, onWithdraw: noop }),
  );
}

function renderAsk(
  r: FdAskReadingVM | null,
  pledgeList: { actor: { badge: string }; at: string }[] = [],
  quotedInBriefs: { id: string; kind: string }[] = [],
): string {
  return renderToStaticMarkup(
    createElement(FdAskPage, {
      ask,
      reading: r,
      pledgeList,
      quotedInBriefs,
      onPledge: noop,
      onWithdraw: noop,
      backHref: "/foundry/exchange",
    }),
  );
}

describe("the board card's reading chip", () => {
  it("links to the reading section of the ask's page, label and text unchanged", () => {
    const html = renderCard(ask);
    expect(html).toContain(
      'href="/foundry/exchange/ask-924f331f1bede6dc#reading"',
    );
    expect(html).toContain("fd-cellchip--link");
    // The date is now a machine-readable <time> (bundle-diet/gloss sweep),
    // so the label is checked as text around the tag rather than one raw run.
    expect(html).toContain("Game clubs ·");
    expect(html).toContain('<time dateTime="2026-08-17">2026-08-17</time>');
    expect(html).toContain(
      "the full reading is on the ask&#x27;s page",
    );
  });

  it("keeps the honest not-read line, chipless, for a null reading", () => {
    const html = renderCard({ ...ask, reading: null });
    expect(html).not.toContain("fd-cellchip");
    expect(html).toContain("Not yet read against the deck");
  });
});

describe("the ask page's reading section", () => {
  it("is the #reading anchor the chips point at", () => {
    expect(renderAsk(reading)).toContain('id="reading"');
  });

  it("links open ground to the shelf itself — never a #cell anchor the shelf does not emit", () => {
    const html = renderAsk(reading);
    expect(html).toContain('href="/foundry/play"');
    expect(html).not.toContain("#cell-");
  });

  it("keeps the plain shelf link when the reading fits no cell", () => {
    const html = renderAsk({ ...reading, cell: null, crowdRange: null });
    expect(html).toContain('href="/foundry/play"');
    expect(html).not.toContain("#cell-");
  });

  it("cites the deck range by link and states the pledge count exactly once", () => {
    const html = renderAsk(reading, [
      { actor: { badge: "1a2b" }, at: "2026-08-17T10:00:00.000Z" },
    ]);
    expect(html).toContain('href="/foundry/deck#slide-09"');
    expect(html).toContain("deck slide 09");
    // The card's pledge chip carries the count; the list below is the roll of
    // who pledged, and no third sentence restates either.
    expect(html.match(/\d+ pledges?\b/g)).toHaveLength(1);
    expect(html).not.toContain("Pledged here so far");
    expect(html).not.toContain("reachable community");
  });

  it("links each doc grounded on this ask, the whole line the link", () => {
    const html = renderAsk(reading, [], [
      { id: "open-ground-game-clubs-v1", kind: "brief" },
    ]);
    expect(html).toContain('href="/foundry/gdd/open-ground-game-clubs-v1"');
    expect(html).toContain("brief");
    expect(html).toContain("is grounded on this ask →");
  });

  it("names the kind honestly — a non-brief doc reads as a design doc", () => {
    const html = renderAsk(reading, [], [{ id: "some-doc", kind: "shortgdd" }]);
    expect(html).toContain("design doc");
    expect(html).toContain("is grounded on this ask →");
  });

  it("renders no doc line when no stored key names this ask", () => {
    expect(renderAsk(reading)).not.toContain("grounded on this ask");
  });
});
