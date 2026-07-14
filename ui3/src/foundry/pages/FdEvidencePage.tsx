import { plural } from "../fmt";
import FdSection, { FD_UNAVAILABLE, FdPageHead } from "../components/FdSection";
import "./fdevidence.css";

export type FdEvidenceShot = { name: string; url: string };

export type FdEvidencePageProps = {
  runId: string;
  /** The game this run played, when its scene names a registry row. */
  title?: string | null;
  gameHref?: string | null;
  /** Non-leaking evidence label (basename · short hash); the absolute host
   *  path never reaches this page. */
  label: string | null;
  /** Whether the recorded directory still exists on the operator host. */
  present: boolean;
  shots: readonly FdEvidenceShot[];
  logTail: readonly string[] | null;
  /** Total run.log lines, so a truncated tail says so; null when no log. */
  logLines: number | null;
  /** The whole log file, served through the same path redaction as the tail. */
  logHref: string | null;
  dataSummary: readonly { key: string; value: string }[] | null;
  replayHref: string | null;
  backHref: string;
  /** The record could not be read on this deployment. */
  unavailable?: boolean;
  /** No run carries this id. */
  missing?: boolean;
};

export default function FdEvidencePage({
  runId,
  title = null,
  gameHref = null,
  label,
  present,
  shots,
  logTail,
  logLines,
  logHref,
  dataSummary,
  replayHref,
  backHref,
  unavailable = false,
  missing = false,
}: FdEvidencePageProps) {
  const crumbs = <a href={backHref}>← All runs</a>;

  if (unavailable || missing) {
    return (
      <div className="fd-stack fd-evidence">
        <FdPageHead eyebrow="Evidence" title="Run evidence" crumbs={crumbs} />
        <p className="fd-empty">
          {unavailable ? FD_UNAVAILABLE : "No run on record carries this id."}
        </p>
      </div>
    );
  }

  return (
    <div className="fd-stack fd-evidence">
      <FdPageHead eyebrow="Evidence" title={title ?? "Run evidence"} crumbs={crumbs} />

      <FdSection title="This run">
        <dl className="fd-facts">
          <div>
            <dt>Run</dt>
            <dd className="fd-mono">
              {replayHref ? <a href={replayHref}>{runId}</a> : runId}
            </dd>
          </div>
          {title && gameHref ? (
            <div>
              <dt>Game</dt>
              <dd>
                <a href={gameHref}>{title}</a>
              </dd>
            </div>
          ) : null}
          <div>
            <dt>Recorded at</dt>
            <dd className="fd-mono">{label ?? "no evidence recorded"}</dd>
          </div>
        </dl>
        {label !== null && !present ? (
          <p className="fd-note">
            That directory is no longer present on the operator host.
          </p>
        ) : null}
      </FdSection>

      {label !== null && present ? (
        <>
          <FdSection
            title="Captured frames"
            badge={
              shots.length > 0 ? (
                <span className="fd-chip">{plural(shots.length, "frame")}</span>
              ) : undefined
            }
          >
            {shots.length === 0 ? (
              <p className="fd-empty">No frames in this directory.</p>
            ) : (
              <ul className="fd-evidence__shots">
                {shots.map((shot) => (
                  <li key={shot.name} className="fd-evidence__shot">
                    <a href={shot.url}>
                      <img src={shot.url} alt={`Captured frame ${shot.name}`} loading="lazy" />
                    </a>
                    <span className="fd-evidence__shotname fd-mono">{shot.name}</span>
                  </li>
                ))}
              </ul>
            )}
          </FdSection>

          {logTail ? (
            <FdSection
              title="Runner log"
              aside={
                logHref ? (
                  <a className="fd-chip fd-chip--mono" href={logHref}>
                    run.log
                  </a>
                ) : (
                  <span className="fd-chip fd-chip--mono">run.log</span>
                )
              }
              sub={
                logLines !== null && logLines > logTail.length
                  ? `The last ${logTail.length} of ${logLines} lines.`
                  : undefined
              }
            >
              <pre className="fd-code fd-evidence__log">{logTail.join("\n")}</pre>
            </FdSection>
          ) : null}

          {dataSummary ? (
            <FdSection
              title="Run data"
              aside={<span className="fd-chip fd-chip--mono">data.json</span>}
            >
              <dl className="fd-facts">
                {dataSummary.map((row) => (
                  <div key={row.key}>
                    <dt className="fd-label">{row.key}</dt>
                    <dd>{row.value}</dd>
                  </div>
                ))}
              </dl>
            </FdSection>
          ) : null}
        </>
      ) : null}
    </div>
  );
}
