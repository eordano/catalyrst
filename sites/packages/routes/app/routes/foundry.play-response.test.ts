import { describe, expect, it } from "vitest";

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import FdResponse, { type FdResponseProps } from "@ui/foundry/pages/FdResponse";

// The response page's honesty contract: a signal that does not exist renders as
// its stated absence with the reason — never as a bare 0 pretending relevance —
// and every absence names where the signal would come from.

function props(overrides: Partial<FdResponseProps>): FdResponseProps {
  return {
    title: "Flag Tag",
    slug: "flagtag",
    gameHref: "/foundry/play/flagtag",
    measuredSince: "15 Aug 2026",
    signals: null,
    gatherings: [],
    runs: [],
    marketCell: null,
    emotionalJobs: null,
    cellGaps: null,
    jobGaps: null,
    askAnswers: [],
    gddHref: null,
    memory: [],
    hasVisitorNote: false,
    revision: { kind: "none" },
    ...overrides,
  };
}

function render(overrides: Partial<FdResponseProps> = {}): string {
  return renderToStaticMarkup(createElement(FdResponse, props(overrides)));
}

describe("FdResponse absences", () => {
  it("states every absence with its reason instead of a zero", () => {
    const html = render();
    expect(html).toContain("Visit counts are not connected yet.");
    expect(html).toContain("Replay and download counts are not connected yet.");
    expect(html).toContain("No gathering has been scheduled for this game.");
    expect(html).toContain("No ask on the exchange names this game");
    expect(html).toContain("No design doc is linked to this game.");
    expect(html).toContain("No recorded bot run for this game.");
    expect(html).toContain("No visitor has left a note.");
    expect(html).toContain("No revision has shipped since measurement began.");
    // No absent signal is rendered as a bare zero.
    expect(html).not.toMatch(/>0</);
    expect(html).not.toContain("0 pledges");
    expect(html).not.toContain("0 visitors");
  });

  it("links each absence to where the signal lives", () => {
    const html = render();
    expect(html).toContain('href="/foundry/sessions"');
    expect(html).toContain('href="/foundry/exchange"');
    expect(html).toContain('href="/foundry/play/flagtag"');
  });

  it("names the three sections by the promise's clauses, plus the tool strip", () => {
    const html = render();
    expect(html).toContain("Who came back");
    expect(html).toContain("What mattered");
    expect(html).toContain("What changed after revision");
    expect(html).toContain("Market-making tools");
  });

  it("never renders the jargon words the reader was not given", () => {
    const html = render({
      signals: {
        visits: { days: [], totalEvents: 0, distinctVisitors: 0 },
        replays: [],
        downloads: 0,
      },
    });
    for (const banned of ["Creator Response Analytics", "CRA", "bench fail", "telemetry"]) {
      expect(html).not.toContain(banned);
    }
  });
});

describe("FdResponse measured signals", () => {
  const signals = {
    visits: {
      days: [
        { day: "2026-08-15", visitors: 2, returning: 0 },
        { day: "2026-08-16", visitors: 2, returning: 1 },
      ],
      totalEvents: 5,
      distinctVisitors: 3,
    },
    replays: [
      {
        trajectoryId: "traj-flagtag-arena",
        opens: 4,
        interactions: 8,
        ranAt: "2026-08-15T01:56:01.668Z",
        sandbox: true,
      },
    ],
    downloads: 0,
  };

  it("shows the measured days with the measured-since line and the volume honesty line", () => {
    const html = render({ signals });
    expect(html).toContain("Measured since 15 Aug 2026");
    expect(html).toContain("Aug 15");
    expect(html).toContain("2 sessions");
    expect(html).toContain("1 returning");
    expect(html).toContain("3 distinct browser sessions");
    expect(html).toContain("cannot yet");
    expect(html).toContain("Too few visits yet to read a pattern.");
  });

  it("a measured zero says what was measured, not a bare number", () => {
    const html = render({ signals });
    expect(html).toContain("No one has downloaded the scene-memory bundle");
  });

  it("replay counts link to the replay itself", () => {
    const html = render({ signals });
    expect(html).toContain("opened 4 times");
    expect(html).toContain('href="/foundry/console/trajectories/traj-flagtag-arena"');
  });

  it("the before/after split renders only as the plain sentence it is", () => {
    const html = render({
      signals,
      revision: { kind: "split", deployedDay: "2026-08-16", before: 2, after: 3 },
    });
    expect(html).toContain("Since the deploy on");
    expect(html).toContain("3 sessions");
    expect(html).not.toContain("No revision has shipped");
  });
});

describe("FdResponse readings and gaps", () => {
  const marketCell = {
    cell: "creator-led-social-competition" as const,
    rationale: "Perpetual rounds; score is server-credited seconds carried.",
    confidence: "evidence-backed" as const,
    classifiedAt: "2026-08-16",
    basis: "This program's own reading — not a fact the deployment entity carries.",
  };

  it("reuses the reading verbatim with its provenance", () => {
    const html = render({ marketCell });
    expect(html).toContain("Social competition");
    expect(html).toContain("server-credited seconds carried");
    expect(html).toContain("2026-08-16");
    expect(html).toContain("evidence-backed");
  });

  it("renders the program-level gaps as the what-to-build-next pointer", () => {
    const html = render({
      cellGaps: ["community-operated-game-clubs"],
      jobGaps: ["D"],
    });
    expect(html).toContain("Open ground across all the games");
    expect(html).toContain("Game clubs");
    expect(html).toContain("Become someone else safely");
  });

  it("an unread registry renders no gap list — a gap nobody measured is not a gap", () => {
    const html = render({ cellGaps: null, jobGaps: null });
    expect(html).not.toContain("Open ground across all the games");
  });

  it("an honest 'serves none' read renders its rationale, not an empty chip row", () => {
    const html = render({
      emotionalJobs: [
        {
          job: null,
          rationale: "No persistent state, recognition or shared construction.",
          confidence: "evidence-backed",
          readAt: "2026-08-16",
          basis: "This program's own reading.",
        },
      ],
    });
    expect(html).toContain("none of the six jobs");
    expect(html).toContain("No persistent state, recognition or shared construction.");
  });
});

describe("FdResponse cell job slots", () => {
  const marketCell = {
    cell: "creator-led-social-competition" as const,
    rationale: "Perpetual rounds; score is server-credited seconds carried.",
    confidence: "evidence-backed" as const,
    classifiedAt: "2026-08-16",
    basis: "This program's own reading — not a fact the deployment entity carries.",
  };
  const jobRead = (job: "E" | "F" | "C") => ({
    job,
    rationale: "Observable machinery in the deployed build.",
    confidence: "evidence-backed" as const,
    readAt: "2026-08-16",
    basis: "This program's own reading.",
  });

  it("a classified game renders the three job slots as chips — covered ones linked, absences stated", () => {
    const html = render({ marketCell, emotionalJobs: [jobRead("E"), jobRead("F")] });
    // The jobs render by name; the deck's internal letters and wave jargon do not.
    expect(html).toContain("A place that remembers us");
    expect(html).toContain("Our actions change the build");
    expect(html).toContain("Rivalry without losing the group");
    expect(html).not.toContain("Qualify to enter T0");
    expect(html).not.toContain("Choose B or C");
    // The citation is a link into the quoted slide, not a bare label.
    expect(html).toContain('href="/foundry/deck#slide-10"');
    // Social competition's set is A + C + E: A and C honestly uncovered (each
    // absence stated once in the chip's title and once for a screen reader), E
    // covered and linked to the reading on the game page.
    const absences = html.match(/no observed design serves this yet/g) ?? [];
    expect(absences.length).toBe(4);
    expect(html).toContain("not served");
    expect(html).toContain('href="/foundry/play/flagtag"');
    expect(html).toContain("read as served");
    // F is read but outside the cell's engineered three.
    expect(html).toContain("Also read, outside this cell");
    expect(html).toContain("A reliable place to show up");
  });

  it("an unclassified game keeps the flat job list — no cell slots apply", () => {
    const html = render({
      marketCell: { ...marketCell, cell: null },
      emotionalJobs: [jobRead("C")],
    });
    expect(html).not.toContain("Qualify to enter T0");
    expect(html).toContain(
      "none of the three engineered jobs the deck assigns per cell",
    );
    expect(html).toContain('<a href="/foundry/deck#slide-10">deck slide 10</a>');
    expect(html).toContain("Our actions change the build");
  });
});

describe("FdResponse ask answers", () => {
  it("an ask the reading names renders the demand link with its provenance", () => {
    const html = render({
      askAnswers: [
        {
          requestId: "ask-0123456789abcdef",
          title: "What are you actively playing on Decentraland?",
          readAt: "2026-08-17",
        },
      ],
    });
    expect(html).toContain("Read as the shelf");
    expect(html).toContain('href="/foundry/exchange/ask-0123456789abcdef"');
    expect(html).toContain("What are you actively playing on Decentraland?");
    expect(html).toContain("2026-08-17");
    // The honesty line: the link is this program's reading, not the asker's.
    expect(html).toContain("not the asker");
    expect(html).not.toContain("No ask on the exchange names this game");
  });

  it("a game no reading names keeps the program-wide absence line verbatim", () => {
    const html = render();
    expect(html).toContain("No ask on the exchange names this game");
    expect(html).not.toContain("Read as the shelf");
  });
});
