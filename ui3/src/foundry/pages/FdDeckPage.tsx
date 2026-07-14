import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import "../components/fdtable.css";
import "./fddeck.css";

// The passages of the working-session deck this site cites, quoted verbatim
// from the archived capture — deck voice inside the quote bars, site voice
// outside them. Source typos ("effectivness", the archived "[sic]") are kept as
// captured; the two tabular slides render as the deck's own table and list.

const CAPTURE = "archived capture, read Aug 16, 2026 — source typos are preserved as captured";

const COVERAGE =
  "Slides 09, 10, 12, 13 and 15 of the deck's 19. Nothing else from the deck is reproduced here.";

const CELLS: readonly { cell: string; jobs: readonly string[] }[] = [
  {
    cell: "Social competition",
    jobs: ["A (remembers us)", "C (actions change build)", "E (rivalry w/o loss)"],
  },
  {
    cell: "Community game clubs",
    jobs: ["A (remembers us)", "B (matter w/o fame)", "F (reliable show-up)"],
  },
  {
    cell: "Build-and-play labs",
    jobs: ["A (remembers us)", "C (actions change build)", "D (someone else)"],
  },
];

/** The deck's six tools in its own order, each against the surface this site
 *  actually serves — or nothing, where it serves none. */
const TOOLS: readonly { name: string; href?: string; label: string; note?: string }[] = [
  { name: "Demand Signal", href: "/foundry/exchange", label: "the exchange" },
  { name: "Creator Response Analytics", href: "/foundry/play", label: "the games" },
  { name: "First Player Network", label: "not built" },
  { name: "Clip-to-Play Acquisition", label: "not built" },
  { name: "Community Import", label: "not built" },
  {
    name: "Session Fill",
    href: "/foundry/sessions",
    label: "the calendar",
    note: "the calendar only — nothing routes players to a session",
  },
];

export default function FdDeckPage() {
  return (
    <div className="fd-page fd-stack fd-deck">
      <FdPageHead
        title="The deck"
        intro="Scott McCarthy's “Decentraland: From Thesis to Working Strategy” — advisor input, not adopted strategy."
        aside={
          <span className="fd-chiprow">
            <FdTime iso="2026-08-14" className="fd-chip">
              prepared Aug 14, 2026
            </FdTime>
            <FdTime iso="2026-08-16" className="fd-chip" title={CAPTURE}>
              captured Aug 16, 2026
              <span className="u-sr-only"> — {CAPTURE}</span>
            </FdTime>
            <span className="fd-chip" title={COVERAGE}>
              5 of 19 slides
              <span className="u-sr-only"> — {COVERAGE}</span>
            </span>
          </span>
        }
      />

      <FdSection id="slide-09" title="09 | Market-cell portfolio">
        <blockquote className="fd-deck__quote">
          <p>
            THREE GAMING CELLS test three different reasons to return to the
            same PERSISTENT foundation.
          </p>
          <p className="fd-deck__omit">[…]</p>
          <p>
            Cell 1: CREATOR-LED SOCIAL COMPETITION — “Compete, laugh, earn
            recognition and return to face the same people again.” PROVES:
            fastest route to density, repeat rounds, clip effectivness and
            shareable moments. 6–24 active players + spectators. Creator hosts
            and revises; DCL supplies queue, first players and measurement.
          </p>
          <p>
            Cell 2: COMMUNITY-OPERATED GAME CLUBS — “This is where my people
            gather, and there is always something to do together.” PROVES:
            imported community demand, routine, membership and off-session
            continuity. 8–50 recurring participants. Operator brings/hosts
            community; DCL imports roles, schedules and activity modules.
          </p>
          <p>
            Cell 3: COLLABORATIVE BUILD-AND-PLAY LABS — “Change the world while
            inside it — with other people watching, helping and responding.”
            PROVES: creation as play and the path from player to contributor to
            creator. 2–12 contributors + observers. Creator directs build; DCL
            + players provides [sic] live editing, versioning, attribution plus
            playtest tools worth experiencing live.
          </p>
        </blockquote>
      </FdSection>

      <FdSection id="slide-10" title="10 | Emotional white space">
        <blockquote className="fd-deck__quote">
          <p>
            Each market cell must intentionally design for three emotional
            jobs; persistence is non-negotiable.
          </p>
          <p className="fd-deck__omit">[…]</p>
          <p>1 REQUIRED FOUNDATION — A: A place that remembers us.</p>
          <p>
            2 REQUIRED RECIPROCITY — Choose B or C. (B: I matter here without
            fame · C: Our actions change the build.)
          </p>
          <p>
            3 CELL SIGNATURE — Choose D, E or F. (D: Become someone else safely
            · E: Rivalry without losing the group · F: A reliable place to show
            up.)
          </p>
        </blockquote>

        <figure className="fd-deck__figure">
          <figcaption className="fd-subhead">Per-cell assignment</figcaption>
          <FdScrollTable ariaLabel="Per-cell assignment">
            <table className="fd-table">
              <thead>
                <tr>
                  <th scope="col">Cell</th>
                  <th scope="col">Engineered jobs</th>
                </tr>
              </thead>
              <tbody>
                {CELLS.map((row) => (
                  <tr key={row.cell}>
                    <th scope="row">{row.cell}</th>
                    <td>
                      <span className="fd-chiprow">
                        {row.jobs.map((job) => (
                          <span key={job} className="fd-chip">
                            {job}
                          </span>
                        ))}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </FdScrollTable>
        </figure>

        <blockquote className="fd-deck__quote">
          <p>
            QUALIFY TO ENTER T0: Design and instrument all three jobs. PASS THE
            CELL: A and the assigned signature job must show behavioral lift;
            the B/C comparator remains measured.
          </p>
        </blockquote>
      </FdSection>

      <FdSection
        id="slide-12"
        title="12 | Pilot portfolio"
        sub="T0 — the first test wave — is sized here."
      >
        <blockquote className="fd-deck__quote">
          <p>
            Creators bring the unmet play deficit; Decentraland validates it
            and supplies the test environment.
          </p>
          <p className="fd-deck__omit">[…]</p>
          <p>
            30–36 CREATOR TEAMS · 1,800–3,000 ACTIVATED ADULTS · 6–60 PEOPLE
            PER LIVE CELL · 12–16 WEEKS
          </p>
          <p className="fd-deck__omit">[…]</p>
          <p>
            “Reachable recurring community” means creator-controlled access or
            a DCL-matched pledged cohort. Followers alone do not count.
          </p>
          <p className="fd-deck__omit">[…]</p>
          <p>Ranges are planning hypotheses, not market forecasts.</p>
        </blockquote>
      </FdSection>

      <FdSection id="slide-13" title="13 | Market-making layer">
        <blockquote className="fd-deck__quote">
          <p>
            Six shared market-making tools converge on one outcome: playable
            demand.
          </p>
          <p className="fd-deck__omit">[…]</p>
        </blockquote>

        <figure className="fd-deck__figure">
          <figcaption className="fd-subhead">The six tools, in the deck&rsquo;s order</figcaption>
          <ol className="fd-deck__tools">
            {TOOLS.map((tool) => (
              <li key={tool.name}>
                {tool.name}
                {tool.href ? (
                  <a className="fd-chip" href={tool.href} title={tool.note}>
                    {tool.label}
                    {tool.note ? <span className="u-sr-only"> — {tool.note}</span> : null}
                  </a>
                ) : (
                  <span className="fd-chip">{tool.label}</span>
                )}
              </li>
            ))}
          </ol>
        </figure>
      </FdSection>

      <FdSection id="slide-15" title="15 | AI acceleration">
        <blockquote className="fd-deck__quote">
          <p>
            AI should push every stage toward real human response — without
            taking authorship or faking demand.
          </p>
          <p className="fd-deck__omit">[…]</p>
        </blockquote>

        <figure className="fd-deck__figure">
          <figcaption className="fd-subhead">Stage 1 of the slide&rsquo;s eight</figcaption>
          <dl className="fd-facts">
            <div>
              <dt>Stage</dt>
              <dd>1 CREATORS DEFINE PLAY</dd>
            </div>
            <div>
              <dt>AI job</dt>
              <dd>
                Mine community conversation, cluster unmet needs, draft
                emotional-job briefs and falsifiers.
              </dd>
            </div>
            <div>
              <dt>Ambition / speed</dt>
              <dd>10–20 min</dd>
            </div>
            <div>
              <dt>Human / DCL boundary</dt>
              <dd>Creator owns premise, values and intended human behavior.</dd>
            </div>
          </dl>
        </figure>

        <blockquote className="fd-deck__quote">
          <p>
            Guardrails: no auto-publish • no synthetic audience masquerading as
            demand • no unconsented training • provenance by default.
          </p>
        </blockquote>
      </FdSection>
    </div>
  );
}
