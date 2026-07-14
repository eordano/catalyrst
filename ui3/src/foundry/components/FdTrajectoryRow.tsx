import "./fdtrajectoryrow.css";

export type FdFinishReason = { kind: string; detail?: string };

export type FdTrajectoryRowVM = {
  id: string;
  sceneTitle?: string | null;
  sceneId?: string | null;
  provenance: "bot" | "visitor";
  runner: string | null;
  events: number;
  finishReason: FdFinishReason | null;
  parentTrajectoryId: string | null;
  seedLength: number | null;
  createdAt: string;
};

const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

export function stamp(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}, ${pad(d.getUTCHours())}:${pad(
    d.getUTCMinutes(),
  )} UTC`;
}

export function shortId(id: string): string {
  return id.length > 16 ? id.slice(0, 16) + "…" : id;
}

export function replayHref(id: string): string {
  return `/foundry/console/trajectories/${encodeURIComponent(id)}`;
}

/** One episode in the list. The id opens its replay. */
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
        <a
          className="fd-traj__link fd-table__mono"
          href={replayHref(record.id)}
          title={record.id}
          onClick={onOpen}
        >
          {shortId(record.id)}
        </a>
        {record.parentTrajectoryId ? (
          <span className="fd-table__sub">
            forked from{" "}
            <a
              className="fd-traj__parent"
              href={replayHref(record.parentTrajectoryId)}
              title={record.parentTrajectoryId}
            >
              {shortId(record.parentTrajectoryId)}
            </a>
            {record.seedLength === null ? null : ` at seq ${record.seedLength - 1}`}
          </span>
        ) : null}
      </th>
      <td>
        {record.sceneTitle ?? record.sceneId ?? (
          <span className="fd-traj__none">—</span>
        )}
      </td>
      <td>
        <span className="fd-traj__prov">{record.provenance}</span>
        {record.runner ? <span className="fd-table__sub">{record.runner}</span> : null}
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
      <td className="fd-table__mono" title={record.createdAt}>
        {stamp(record.createdAt)}
      </td>
    </tr>
  );
}
