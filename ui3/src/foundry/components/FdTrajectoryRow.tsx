import { stampUTC as stamp } from "../fmt";
import FdTime from "./FdTime";
import "./fdtrajectoryrow.css";

export { stamp };

export type FdFinishReason = { kind: string; detail?: string };

export type FdTrajectoryRowVM = {
  id: string;
  sceneTitle?: string | null;
  sceneId?: string | null;
  /** The game's biography page, when the run's scene names a registry row. */
  gameHref?: string | null;
  provenance: "bot" | "visitor";
  runner: string | null;
  events: number;
  finishReason: FdFinishReason | null;
  parentTrajectoryId: string | null;
  seedLength: number | null;
  /** Non-leaking evidence label (basename · short hash); null when the run
   *  recorded none. */
  evidence?: string | null;
  evidenceHref?: string | null;
  createdAt: string;
};

export function shortId(id: string): string {
  return id.length > 16 ? id.slice(0, 16) + "…" : id;
}

export function replayHref(id: string): string {
  return `/foundry/console/trajectories/${encodeURIComponent(id)}`;
}

/** One run in the list. The name opens its log; the scene opens the game. */
export default function FdTrajectoryRow({
  record,
  onOpen,
}: {
  record: FdTrajectoryRowVM;
  onOpen: () => void;
}) {
  const finish = record.finishReason;
  return (
    <tr className="fd-traj">
      <th scope="row" className="fd-traj__idcell">
        {record.sceneTitle ? (
          <>
            <a className="fd-traj__link" href={replayHref(record.id)} onClick={onOpen}>
              {record.sceneTitle}
            </a>
            <span className="fd-table__sub fd-traj__id" title={record.id}>
              {shortId(record.id)}
              <span className="u-sr-only"> (full id: {record.id})</span>
            </span>
          </>
        ) : (
          <a
            className="fd-traj__link fd-table__mono"
            href={replayHref(record.id)}
            title={record.id}
            onClick={onOpen}
          >
            {shortId(record.id)}
            <span className="u-sr-only"> (full id: {record.id})</span>
          </a>
        )}
        {record.parentTrajectoryId ? (
          <span className="fd-table__sub">
            forked from{" "}
            <a
              className="fd-traj__parent"
              href={replayHref(record.parentTrajectoryId)}
              title={record.parentTrajectoryId}
            >
              {shortId(record.parentTrajectoryId)}
              <span className="u-sr-only"> (full id: {record.parentTrajectoryId})</span>
            </a>
            {record.seedLength === null ? null : ` at #${record.seedLength - 1}`}
          </span>
        ) : null}
      </th>
      <td className="fd-table__mono">
        {record.sceneId === null || record.sceneId === undefined ? (
          <span className="fd-traj__none">—</span>
        ) : record.gameHref ? (
          <a href={record.gameHref}>{record.sceneId}</a>
        ) : (
          record.sceneId
        )}
      </td>
      <td>
        {record.runner === "arena" ? (
          // The arena-row rule: the raw runner value never surfaces as prose —
          // the chip carries the visitor-facing word, the title the stored one.
          <span className="fd-chip" title={`runner: ${record.runner}`}>
            sandbox
            <span className="u-sr-only"> (stored value: arena)</span>
          </span>
        ) : record.runner === "dclbots" ? (
          <span className="fd-chip" title={`runner: ${record.runner}`}>
            playtest harness
            <span className="u-sr-only"> (stored value: dclbots)</span>
          </span>
        ) : record.runner ? (
          <span className="fd-chip">{record.runner}</span>
        ) : (
          <span className="fd-chip" title={`provenance: ${record.provenance}`}>
            {record.provenance}
          </span>
        )}
      </td>
      <td className="fd-table__num">{record.events}</td>
      <td>
        {finish ? (
          <span className={"fd-traj__finish is-" + finish.kind}>{finish.kind}</span>
        ) : (
          <span className="fd-traj__none">not recorded</span>
        )}
        {finish?.detail ? <span className="fd-table__sub">{finish.detail}</span> : null}
      </td>
      <td className="fd-table__mono">
        {record.evidence ? (
          record.evidenceHref ? (
            <a href={record.evidenceHref}>{record.evidence}</a>
          ) : (
            record.evidence
          )
        ) : (
          <span className="fd-traj__none">none</span>
        )}
      </td>
      <td className="fd-table__mono">
        <FdTime iso={record.createdAt} title={record.createdAt}>
          {stamp(record.createdAt)}
        </FdTime>
      </td>
    </tr>
  );
}
