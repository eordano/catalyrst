import type { FdMarketCellSlug } from "./FdGameCard";
import "./fdcellchip.css";
import "./fdemotionaljobs.css";

// The six emotional jobs of the strategy deck's slide "10 | EMOTIONAL WHITE
// SPACE" (11.10, Scott McCarthy working-strategy deck).
export type FdEmotionalJobLetter = "A" | "B" | "C" | "D" | "E" | "F";

/** Display names for the job letters — a display mapping, not data (deck slide
 *  10, 11.10 lines 305-315). */
export const JOB_NAMES: Record<FdEmotionalJobLetter, string> = {
  A: "A place that remembers us",
  B: "I matter here without fame",
  C: "Our actions change the build",
  D: "Become someone else safely",
  E: "Rivalry without losing the group",
  F: "A reliable place to show up",
};

/** The deck's slide-10 assignment table: the job set each cell engineers —
 *  always [A, the cell's B/C pick, the D/E/F signature job]. A display
 *  mapping, not data. */
export const SIGNATURE_JOBS: Record<
  FdMarketCellSlug,
  readonly [FdEmotionalJobLetter, FdEmotionalJobLetter, FdEmotionalJobLetter]
> = {
  "creator-led-social-competition": ["A", "C", "E"],
  "community-operated-game-clubs": ["A", "B", "F"],
  "collaborative-build-and-play-labs": ["A", "C", "D"],
};

/** One row of this program's reading: a job the game's observable design
 *  serves, or (job null) the honest "read, serves none" verdict. */
export type FdEmotionalJobVM = {
  job: FdEmotionalJobLetter | null;
  rationale: string;
  confidence: "evidence-backed" | "inferred";
  readAt: string;
  basis: string;
};

/** The chip's provenance affordance — mirrors marketCellTitle exactly. */
export function jobTitle(r: FdEmotionalJobVM): string {
  return `This program's reading, ${r.readAt} — ${r.rationale} (${r.confidence})`;
}

export type FdEmotionalJobsProps = {
  reads: readonly FdEmotionalJobVM[];
  /** The game's market-cell slug when its cell reading placed it in one; null
   *  covers both "read as unclassified" and "cell never read" — `cellRead`
   *  tells those apart, so the gap copy never invents a cell verdict. */
  cell: FdMarketCellSlug | null | undefined;
  /** True when a market-cell row exists at all (even an unclassified one). */
  cellRead: boolean;
};

function Legend() {
  return (
    <details className="fd-emojobs__legend">
      <summary>The deck&rsquo;s six jobs</summary>
      <ul>
        <li>A — A place that remembers us (required in every cell)</li>
        <li>B — I matter here without fame (B/C: the reciprocity pair)</li>
        <li>C — Our actions change the build (B/C: the reciprocity pair)</li>
        <li>D — Become someone else safely (cell signature)</li>
        <li>E — Rivalry without losing the group (cell signature)</li>
        <li>F — A reliable place to show up (cell signature)</li>
      </ul>
    </details>
  );
}

export default function FdEmotionalJobs({ reads, cell, cellRead }: FdEmotionalJobsProps) {
  const first = reads[0];
  if (!first) {
    return (
      <p className="fd-panelnote">
        Not yet read against the deck&rsquo;s six emotional jobs.
      </p>
    );
  }

  const provenance = (
    <p className="fd-panelnote">
      {first.basis} Read {first.readAt}.
    </p>
  );

  const jobReads = reads.filter(
    (r): r is FdEmotionalJobVM & { job: FdEmotionalJobLetter } => r.job !== null,
  );
  const noneRead = reads.find((r) => r.job === null);

  if (jobReads.length === 0 && noneRead) {
    return (
      <div className="fd-emojobs">
        <p className="fd-emojobs__none">
          Read {noneRead.readAt}: none of the six jobs&rsquo; observable machinery is
          present here.
        </p>
        <p className="fd-gamedetail__cellrationale">{noneRead.rationale}</p>
        {provenance}
        <Legend />
      </div>
    );
  }

  // The gap line is derived from the rows and the deck's assignment table
  // alone — never typed per game.
  let gap = null;
  if (cell) {
    const set = SIGNATURE_JOBS[cell];
    const sig = set[2];
    const served = new Set(jobReads.map((r) => r.job));
    const gaps = set.filter((j) => !served.has(j));
    gap =
      gaps.length > 0 ? (
        <p className="fd-emojobs__gap">
          Not yet served from its cell&rsquo;s engineered set ({set.join(" + ")}):{" "}
          {gaps.map((j) => `${j} — ${JOB_NAMES[j].toLowerCase()}`).join("; ")}. The
          deck&rsquo;s pass rule: A and the signature job ({sig}) must show behavioral
          lift.
        </p>
      ) : (
        <p className="fd-emojobs__gap">
          Every job its cell engineers ({set.join(" + ")}) has observed design serving
          it; behavioral lift is a measurement this page does not claim.
        </p>
      );
  } else if (cellRead) {
    gap = (
      <p className="fd-emojobs__gap">
        Unclassified games carry no engineered job set to fall short of — the jobs
        above are read on their own terms.
      </p>
    );
  }

  return (
    <div className="fd-emojobs">
      <div className="fd-emojobs__chips">
        {jobReads.map((r) => (
          <span
            key={r.job}
            className="fd-cellchip fd-emojobs__chip"
            title={jobTitle(r)}
            aria-label={jobTitle(r)}
          >
            {r.job} · {JOB_NAMES[r.job]} · {r.readAt}
          </span>
        ))}
      </div>
      {gap}
      {provenance}
      <Legend />
    </div>
  );
}
