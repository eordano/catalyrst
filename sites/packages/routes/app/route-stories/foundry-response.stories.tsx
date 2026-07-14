import FoundryGameResponse from "../routes/foundry.play_.$slug_.response";
import emotionalJobs from "@data/fixtures/foundry-emotional-jobs.json";
import marketCells from "@data/fixtures/foundry-market-cells.json";
import type { FdEmotionalJobVM } from "@ui/foundry/components/FdEmotionalJobs";
import type { FdMarketCellVM } from "@ui/foundry/components/FdGameCard";
import { expect, waitFor } from "@ui/docs/sb";
import { routeStory } from "./lib";

// The per-game response page: the deck's "who returned, what mattered, what
// changed", scoped to what the program measures today. The readings come from
// the same fixture the importer lands — the judgment text is never typed here.

const cellRow = marketCells.find((r) => r.sceneId === "flagtag");
const cell: FdMarketCellVM | null = cellRow
  ? ({
      cell: cellRow.cell,
      rationale: cellRow.rationale,
      confidence: cellRow.confidence,
      classifiedAt: cellRow.classifiedAt,
      basis: cellRow.basis,
    } as FdMarketCellVM)
  : null;

const jobs: FdEmotionalJobVM[] = emotionalJobs
  .filter((r) => r.sceneId === "flagtag")
  .map(
    (r) =>
      ({
        job: r.job,
        rationale: r.rationale,
        confidence: r.confidence,
        readAt: r.readAt,
        basis: r.basis,
      }) as FdEmotionalJobVM,
  );

const base = {
  badge: "ab12",
  unavailable: false,
  slug: "flagtag",
  title: "Flag Tag",
  gameHref: "/foundry/play/flagtag",
  measuredSince: "15 Aug 2026",
  signals: null,
  gatherings: [],
  runs: [
    {
      id: "bench-flagtag-smoke",
      ranAt: "2026-08-10T12:00:00.000Z",
      text: "2 of 2 checks failed",
      evidenceHref: "/foundry/console/evidence/bench-flagtag-smoke",
      replayHref: "/foundry/console/trajectories/traj-flagtag-smoke",
    },
    {
      id: "bench-flagtag-arena",
      ranAt: "2026-08-09T10:00:00.000Z",
      text: "completed a sandbox simulation",
      evidenceHref: "/foundry/console/evidence/bench-flagtag-arena",
      replayHref: "/foundry/console/trajectories/traj-flagtag-arena",
    },
  ],
  marketCell: cell,
  emotionalJobs: jobs,
  cellGaps: ["community-operated-game-clubs"],
  jobGaps: ["B", "D", "F"],
  askAnswers: [],
  gddHref: null,
  memory: [
    {
      eventId: "c1",
      at: "2026-07-05T19:52:10.768Z",
      body: "Deployed to flagtag.dcl.eth",
      sourceNote: "worlds mirror row for flagtag.dcl.eth",
    },
  ],
  hasVisitorNote: false,
  revision: { kind: "none" },
};

export default {
  title: "Routes/FoundryResponse",
  parameters: { layout: "fullscreen", a11y: { test: "todo" } },
};

// Counts not connected: every count-bearing line states the absence — no fake
// numbers, no bare zeros pretending relevance.
export const CountsNotConnected = {
  render: routeStory({
    Component: FoundryGameResponse,
    path: "/foundry/play/:slug/response",
    url: "/foundry/play/flagtag/response",
    loaderData: base,
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("Visit counts are not connected yet.");
      expect(canvasElement.textContent).toContain(
        "Replay and download counts are not connected yet.",
      );
      expect(canvasElement.textContent).toContain("No ask on the exchange names this game");
      expect(canvasElement.textContent).toContain("2 of 2 checks failed");
      // The arena rule: a sandbox run never reads as a pass.
      expect(canvasElement.textContent).toContain("completed a sandbox simulation");
      expect(canvasElement.textContent).toContain("Market-making tools");
    });
  },
};

// The deck's slide-10 QUALIFY rule as three slots (flagtag serves only its
// signature job — two honest absences), plus an ask whose reading names this
// game as the shelf's answer.
export const QualifyAndAskAnswer = {
  render: routeStory({
    Component: FoundryGameResponse,
    path: "/foundry/play/:slug/response",
    url: "/foundry/play/flagtag/response",
    loaderData: {
      ...base,
      askAnswers: [
        {
          requestId: "ask-0123456789abcdef",
          title: "What are you actively playing on Decentraland?",
          readAt: "2026-08-17",
        },
      ],
    },
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("Qualify to enter T0");
      expect(canvasElement.textContent).toContain(
        "no observed design serves this yet",
      );
      expect(canvasElement.textContent).toContain("Read as the shelf");
      expect(canvasElement.textContent).toContain(
        "What are you actively playing on Decentraland?",
      );
    });
  },
};

export const MeasuredSignals = {
  render: routeStory({
    Component: FoundryGameResponse,
    path: "/foundry/play/:slug/response",
    url: "/foundry/play/flagtag/response",
    loaderData: {
      ...base,
      signals: {
        visits: {
          days: [
            { day: "2026-08-15", visitors: 2, returning: 0 },
            { day: "2026-08-16", visitors: 2, returning: 1 },
          ],
          totalEvents: 5,
          distinctVisitors: 3,
        },
        replays: [{ trajectoryId: "traj-flagtag-arena", opens: 4, interactions: 8 }],
        downloads: 3,
      },
    },
  }),
  play: async ({ canvasElement }: { canvasElement: HTMLElement }) => {
    await waitFor(() => {
      expect(canvasElement.textContent).toContain("Measured since 15 Aug 2026");
      expect(canvasElement.textContent).toContain("Too few visits yet to read a pattern.");
      expect(canvasElement.textContent).toContain("3 distinct visitors so far.");
      expect(canvasElement.textContent).toContain("opened 4 times");
    });
  },
};
