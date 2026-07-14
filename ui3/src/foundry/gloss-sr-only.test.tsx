import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import FdTrajectoryRow, {
  type FdTrajectoryRowVM,
} from "./components/FdTrajectoryRow";
import FdTrajectoryReplay, {
  type FdReplayHeaderVM,
} from "./pages/FdTrajectoryReplay";
import FdResponse from "./pages/FdResponse";
import FdEventRow, { type FdEventVM } from "./components/FdEventRow";

// The four verified title-only gloss sites (charter item 3): a visible
// truncated/labeled value keeps its hover title, and now carries a u-sr-only
// twin so a screen reader gets the same information without hover — the
// FdTime pattern, applied where FdTime itself doesn't fit.

const noop = () => undefined;

describe("FdTrajectoryRow — full id + arena chip", () => {
  const record: FdTrajectoryRowVM = {
    id: "trj-1234567890abcdef1234567890",
    sceneTitle: "Some Scene",
    sceneId: "scene-1",
    provenance: "bot",
    runner: "arena",
    events: 3,
    finishReason: null,
    parentTrajectoryId: "trj-parentabcdef1234567890abcdef",
    seedLength: 5,
    createdAt: "2026-08-17T00:00:00.000Z",
  };

  it("carries the full trajectory id and parent id as sr-only twins of the truncated, titled link text", () => {
    const html = renderToStaticMarkup(
      createElement(FdTrajectoryRow, { record, onOpen: noop }),
    );
    expect(html).toContain('title="trj-1234567890abcdef1234567890"');
    expect(html).toContain(
      '<span class="u-sr-only"> (full id: trj-1234567890abcdef1234567890)</span>',
    );
    expect(html).toContain('title="trj-parentabcdef1234567890abcdef"');
    expect(html).toContain(
      '<span class="u-sr-only"> (full id: trj-parentabcdef1234567890abcdef)</span>',
    );
  });

  it("gives the arena runner chip a value-only sr-only twin — never prose", () => {
    const html = renderToStaticMarkup(
      createElement(FdTrajectoryRow, { record, onOpen: noop }),
    );
    expect(html).toContain('title="runner: arena"');
    expect(html).toContain('<span class="u-sr-only"> (stored value: arena)</span>');
    // Visible text stays "sandbox" — "arena" appears only in title/sr-only.
    expect(html).toContain(">sandbox<");
  });
});

describe("FdTrajectoryReplay — arena chip", () => {
  const header: FdReplayHeaderVM = {
    id: "trj-1",
    sceneTitle: "Some Scene",
    sceneId: "scene-1",
    provenance: "bot",
    runner: "arena",
    finishReason: null,
    parentTrajectoryId: null,
    seedLength: null,
    evidencePath: null,
    createdAt: "2026-08-17T00:00:00.000Z",
  };

  it("mirrors the dclbots-chip sr-only wording for arena", () => {
    const html = renderToStaticMarkup(
      createElement(FdTrajectoryReplay, {
        header,
        events: [],
        cursor: 0,
        onCursor: noop,
        onStep: noop,
        backHref: "/foundry/console/trajectories",
      }),
    );
    expect(html).toContain('<span class="u-sr-only"> (stored value: arena)</span>');
  });
});

describe("FdResponse — none-read confidence", () => {
  it("gives the honest none-of-six-jobs line its reading provenance as an sr-only twin", () => {
    const html = renderToStaticMarkup(
      createElement(FdResponse, {
        title: "Some Game",
        slug: "some-game",
        gameHref: "/foundry/play/some-game",
        measuredSince: "15 Aug 2026",
        signals: null,
        gatherings: [],
        runs: [],
        marketCell: null,
        emotionalJobs: [
          {
            job: null,
            rationale: "Reads no observable machinery for any of the six.",
            confidence: "inferred",
            readAt: "2026-08-10",
            basis: "basis text",
          },
        ],
        cellGaps: null,
        jobGaps: null,
        askAnswers: [],
        gddHref: null,
        memory: [],
        hasVisitorNote: false,
        revision: { kind: "none" },
      }),
    );
    expect(html).toContain("none of the six jobs");
    expect(html).toContain(
      '<span class="u-sr-only"> — This program&#x27;s reading, 2026-08-10 — Reads no observable machinery for any of the six. (inferred)</span>',
    );
  });
});

describe("FdEventRow — NaN-time raw stored time", () => {
  it("carries the raw stored time as an sr-only twin when the timestamp fails to parse", () => {
    const event: FdEventVM = {
      seq: 0,
      type: "turn/start",
      time: "not-a-timestamp",
      data: {},
      depth: 0,
      offsetMs: null,
      spanMs: null,
    };
    const html = renderToStaticMarkup(createElement(FdEventRow, { event }));
    expect(html).toContain('title="not-a-timestamp"');
    expect(html).toContain('<span class="u-sr-only"> (not-a-timestamp)</span>');
  });

  it("a parseable time keeps using FdTime's own sr twin instead — no double gloss", () => {
    const event: FdEventVM = {
      seq: 0,
      type: "turn/start",
      time: "2026-08-17T00:00:00.000Z",
      data: {},
      depth: 0,
      offsetMs: 1000,
      spanMs: null,
    };
    const html = renderToStaticMarkup(createElement(FdEventRow, { event }));
    expect(html).not.toContain("(not-a-timestamp)");
    expect(html).toContain("<time");
  });
});
