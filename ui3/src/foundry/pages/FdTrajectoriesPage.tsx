import { plural } from "../fmt";
import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FD_UNAVAILABLE, FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import FdTrajectoryRow, { type FdTrajectoryRowVM } from "../components/FdTrajectoryRow";
import "../components/fdstat.css";
import "./fdtrajectories.css";

export type { FdTrajectoryRowVM };

/** A ?scene= filter in force: which rows the counts below reflect. `title` and
 *  `gameHref` are null when the slug names no registry row. */
export type FdTrajectoriesFilterVM = {
  sceneId: string;
  title: string | null;
  gameHref: string | null;
  clearHref: string;
};

export type FdTrajectoriesPageProps = {
  records: readonly FdTrajectoryRowVM[];
  /** Total runs on record; the list below is capped. */
  total?: number;
  filter?: FdTrajectoriesFilterVM | null;
  /** The records could not be read on this deployment. */
  unavailable?: boolean;
  onOpen: (id: string, provenance: "bot" | "visitor") => void;
};

export default function FdTrajectoriesPage({
  records,
  total,
  filter = null,
  unavailable = false,
  onOpen,
}: FdTrajectoriesPageProps) {
  const events = records.reduce((sum, r) => sum + r.events, 0);
  const shown = records.length;
  const totalRuns = total ?? shown;
  const capped = totalRuns > shown;
  const filterLabel = filter ? filter.title ?? filter.sceneId : null;

  if (unavailable) {
    return (
      <div className="fd-stack fd-traj-page">
        <FdPageHead title="Run logs" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return (
    <div className="fd-stack fd-traj-page">
      <FdPageHead title="Run logs" intro="Open a run to step through its events." />

      {filter ? (
        <p className="fd-chiprow fd-traj-page__filter">
          {filter.gameHref ? (
            <a className="fd-chip" href={filter.gameHref}>
              {filterLabel}
            </a>
          ) : (
            <span className="fd-chip">{filterLabel}</span>
          )}
          <a className="fd-traj-page__clear" href={filter.clearHref}>
            Show every run
          </a>
        </p>
      ) : null}

      {shown > 0 ? (
        <div className="fd-statrow">
          <FdStat
            label="Run logs"
            value={totalRuns}
            title="one row per recorded episode — sandbox simulations included, labeled on their rows"
          />
          <FdStat
            label="Events"
            value={events}
            title="logged events across the runs listed below"
          />
        </div>
      ) : null}

      <FdSection
        title={filter ? "Run logs on this game" : "Every run log"}
        badge={
          shown > 0 ? <span className="fd-chip">{plural(shown, "run")}</span> : undefined
        }
      >
        {shown === 0 ? (
          <p className="fd-empty">
            {filter
              ? "No runs recorded for this game."
              : "No runs recorded yet."}
          </p>
        ) : (
          <>
            <FdScrollTable ariaLabel="Recorded runs">
              <table className="fd-table">
                <thead>
                  <tr>
                    <th scope="col">Run</th>
                    <th scope="col">Scene</th>
                    <th scope="col">Runner</th>
                    <th scope="col">Events</th>
                    <th scope="col">Finish</th>
                    <th scope="col">Evidence</th>
                    <th scope="col">Recorded</th>
                  </tr>
                </thead>
                <tbody>
                  {records.map((record) => (
                    <FdTrajectoryRow
                      key={record.id}
                      record={record}
                      onOpen={() => onOpen(record.id, record.provenance)}
                    />
                  ))}
                </tbody>
              </table>
            </FdScrollTable>
            {capped ? (
              <p className="fd-note">
                Showing the {shown} most recent of {totalRuns} runs.
              </p>
            ) : null}
          </>
        )}
      </FdSection>
    </div>
  );
}
