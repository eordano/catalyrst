import EmptyState from "../../components/EmptyState";
import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import FdTrajectoryRow, { type FdTrajectoryRowVM } from "../components/FdTrajectoryRow";
import "../components/fdstat.css";
import "./fdtrajectories.css";

export type { FdTrajectoryRowVM };

export type FdTrajectoriesPageProps = {
  records: readonly FdTrajectoryRowVM[];
  /** Total episodes on record; the list below is capped. */
  total?: number;
  onOpen: (id: string, provenance: "bot" | "visitor") => void;
};

export default function FdTrajectoriesPage({
  records,
  total,
  onOpen,
}: FdTrajectoriesPageProps) {
  const events = records.reduce((sum, r) => sum + r.events, 0);
  const shown = records.length;
  const totalEpisodes = total ?? shown;
  const capped = totalEpisodes > shown;

  return (
    <div className="fd-stack fd-traj-page">
      <FdPageHead
        title="Trajectories"
        intro="Every episode here is an append-only event log recorded from a real run — ingested bot benches today, visitor episodes when a capture surface exists. Replay is re-derivation: the scrubber shows exactly what the log contains, nothing else."
      />

      <div className="fd-statrow">
        <FdStat
          label="Episodes"
          value={capped ? `${shown} of ${totalEpisodes}` : shown}
          note={
            capped
              ? "Showing the most recent runs; the total is a live row count."
              : "One row per recorded run. Row count, nothing derived."
          }
        />
        <FdStat
          label="Events"
          value={events}
          note={
            capped
              ? "Total logged events across the shown episodes."
              : "Total logged events across those episodes."
          }
        />
      </div>

      <FdSection title="Episodes">
        {records.length === 0 ? (
          <EmptyState
            variant="inline"
            title="No episodes recorded."
            subtitle="Episodes arrive from the dcl-scene-bots harness through foundry:ingest-bench. Nothing is generated to fill this table."
          />
        ) : (
          <FdScrollTable ariaLabel="Recorded episodes">
            <table className="fd-table">
              <thead>
                <tr>
                  <th scope="col">Episode</th>
                  <th scope="col">Scene</th>
                  <th scope="col">Provenance</th>
                  <th scope="col">Events</th>
                  <th scope="col">Finish</th>
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
        )}

        <p className="fd-note">
          A finish reason is the last <code>turn/end</code> the log holds. An episode
          that never closed its turn says so instead of being scored.
        </p>
      </FdSection>

    </div>
  );
}
