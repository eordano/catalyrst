import { dayUTC, groupDigits, plural, stampUTC } from "../fmt";
import Button from "../../atoms/Button";
import FdTime from "../components/FdTime";
import FdPersonaChip, { type FdChipActor } from "../components/FdPersonaChip";
import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FD_STEWARDS_SUB, FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import { formatSize, humanizeEntityTitle } from "../components/FdGameCard";
import "../components/fdpersonachip.css";
import "../components/fdtable.css";
import "./fdcontinuity.css";

export type FdContinuityCountsVM = {
  changelog: number;
  /** Real bot runs — sandbox sims excluded, per the arena-row rule. */
  reports: number;
  /** Every stored report, sandbox sims included — what the bundle carries. */
  reportsAll: number;
  episodes: number;
  docs: number;
  stewards: number;
};

export type FdContinuitySceneVM = {
  id: string;
  title: string;
  worldName: string | null;
  deployedAt: string | null;
  importedAt: string | null;
  sizeBytes: number | null;
  parcels: number | null;
  source: "worlds-mirror" | "repo";
  sourceNote: string;
  counts: FdContinuityCountsVM;
  exportHref: string;
};

export type FdContinuityMemoryRowVM = {
  at: string;
  actor: { name: string } | { badge: string } | { source: string };
  action: string;
  body: string;
  sourceNote: string;
};

export type FdContinuityStewardVM = {
  actor: { name: string } | { badge: string };
  basis: string;
  since: string;
  releasedAt: string | null;
  releaseReason: "self" | "transfer" | null;
  viaTransfer: boolean;
};

export type FdContinuityTransferVM = {
  id: string;
  from: { name: string } | { badge: string };
  note: string;
  effectiveStatus: "offered" | "accepted" | "revoked" | "expired";
  createdAt: string;
  expiresAt: string;
  acceptedAt: string | null;
  acceptedBy: { name: string } | { badge: string } | null;
};

export type FdContinuityDetailVM = {
  scene: FdContinuitySceneVM;
  memory: readonly FdContinuityMemoryRowVM[];
  stewards: {
    active: readonly FdContinuityStewardVM[];
    past: readonly FdContinuityStewardVM[];
  };
  transfers: readonly FdContinuityTransferVM[];
};

export type FdContinuityPageProps = {
  scenes: readonly FdContinuitySceneVM[];
  selected: string | null;
  selectedMissing: boolean;
  detail: FdContinuityDetailVM | null;
  /** Fired on a download anchor's click — the click, not the GET, is the
   *  tracked fact (a bare GET must never mint download evidence). */
  onDownload?: (sceneId: string) => void;
};

// Where the two run counts differ, and why — the whole sandbox rule, carried
// on the numbers themselves rather than restated in prose.
const RUNS_TITLE = "stored bench runs, sandbox sims excluded";
const LOGS_TITLE = "every recorded run log, sandbox sims included";

function chip(actor: { name: string } | { badge: string } | { source: string }): FdChipActor {
  return actor;
}

function sceneHref(id: string): string {
  return `?scene=${encodeURIComponent(id)}#scene`;
}

/** A count is a chip only when there is something to count. */
function CountChip({
  n,
  noun,
  title,
}: {
  n: number;
  noun: string;
  title?: string;
}) {
  if (n === 0) return null;
  return (
    <span className="fd-chip fd-num" title={title}>
      {plural(n, noun)}
    </span>
  );
}

function SceneDetail({
  detail,
  onDownload,
}: {
  detail: FdContinuityDetailVM;
  onDownload?: (sceneId: string) => void;
}) {
  const s = detail.scene;
  const stewardCount = detail.stewards.active.length + detail.stewards.past.length;
  return (
    <>
      <FdSection
        id="scene"
        title={humanizeEntityTitle(s.title)}
        aside={<a href="?">Close this scene</a>}
      >
        <div className="fd-statrow">
          <FdStat label="Changelog" value={s.counts.changelog} mono />
          <FdStat label="Runs" value={s.counts.reports} mono title={RUNS_TITLE} />
          <FdStat
            label="Run logs"
            value={s.counts.episodes}
            mono
            title={LOGS_TITLE}
          />
          <FdStat label="Docs" value={s.counts.docs} mono />
          <FdStat label="Active stewards" value={s.counts.stewards} mono />
        </div>
        <div className="fd-panel fd-continuity__bundle">
          <p className="fd-chiprow">
            <CountChip n={s.counts.changelog} noun="changelog entry" />
            <CountChip n={s.counts.reports} noun="run" title={RUNS_TITLE} />
            {s.counts.reportsAll - s.counts.reports > 0 ? (
              <CountChip
                n={s.counts.reportsAll - s.counts.reports}
                noun="sandbox sim"
                title="in the bundle, labeled — never counted as runs"
              />
            ) : null}
            <CountChip n={s.counts.episodes} noun="run log" title={LOGS_TITLE} />
            <CountChip n={s.counts.docs} noun="design doc" />
            <span className="fd-chip">steward history</span>
            <span className="fd-chip">succession offers</span>
            <span className="fd-chip fd-chip--mono">JSON, no scene bytes</span>
          </p>
          <p className="fd-card__foot fd-continuity__actions">
            <Button
              as="a"
              variant="secondary"
              size="sm"
              href={s.exportHref}
              download
              onClick={() => onDownload?.(s.id)}
            >
              Download the bundle
            </Button>
            <a href={`${s.exportHref}?view`}>View it in the browser</a>
          </p>
        </div>
      </FdSection>

      <FdSection
        title="Scene memory"
        badge={
          detail.memory.length > 0 ? (
            <span className="fd-chip">{plural(detail.memory.length, "row")}</span>
          ) : undefined
        }
      >
        {detail.memory.length === 0 ? (
          <p className="fd-empty">Nothing recorded for this scene yet.</p>
        ) : (
          <FdScrollTable ariaLabel="Scene memory">
            <table className="fd-table">
              <thead>
                <tr>
                  <th scope="col">When</th>
                  <th scope="col">Who</th>
                  <th scope="col">What</th>
                  <th scope="col">Source note</th>
                </tr>
              </thead>
              <tbody>
                {detail.memory.map((m, i) => (
                  <tr key={i}>
                    <td className="fd-table__mono">
                      <FdTime iso={m.at} title={m.at}>
                        {stampUTC(m.at)}
                      </FdTime>
                    </td>
                    <td>
                      <FdPersonaChip actor={chip(m.actor)} />
                    </td>
                    <td className="fd-table__prose">
                      <span className="fd-mono">{m.action}</span>
                      {m.body ? <span> {m.body}</span> : null}
                    </td>
                    <td className="fd-table__prose">
                      {m.sourceNote || (
                        <span className="fd-note-inline">none</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </FdScrollTable>
        )}
      </FdSection>

      <FdSection
        title="Stewards"
        sub={FD_STEWARDS_SUB}
        aside={<a href={`/foundry/play/${s.id}`}>Claim a seat →</a>}
      >
        {stewardCount === 0 ? (
          <p className="fd-empty">No steward has claimed this scene.</p>
        ) : (
          <ul className="fd-continuity__stewards">
            {detail.stewards.active.map((st, i) => (
              <li className="fd-continuity__steward" key={`a${i}`}>
                <FdPersonaChip actor={chip(st.actor)} />
                <FdTime
                  iso={st.since}
                  className="fd-note-inline"
                  title={st.since}
                >
                  since {dayUTC(st.since)}
                </FdTime>
                {st.viaTransfer ? <span className="fd-chip">via transfer</span> : null}
                {st.basis ? <p className="fd-continuity__basis">{st.basis}</p> : null}
              </li>
            ))}
            {detail.stewards.past.map((st, i) => (
              <li className="fd-continuity__steward is-past" key={`p${i}`}>
                <FdPersonaChip actor={chip(st.actor)} />
                <FdTime
                  iso={st.since}
                  className="fd-note-inline"
                  title={st.since}
                >
                  {dayUTC(st.since)}
                  {st.releasedAt ? ` – released ${dayUTC(st.releasedAt)}` : ""}
                </FdTime>
                {st.releaseReason ? (
                  <span className="fd-chip">released: {st.releaseReason}</span>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </FdSection>

      <FdSection
        title="Succession"
        badge={
          detail.transfers.length > 0 ? (
            <span className="fd-chip">
              {plural(detail.transfers.length, "offer")}
            </span>
          ) : undefined
        }
      >
        {detail.transfers.length === 0 ? (
          <p className="fd-empty">No succession offer recorded.</p>
        ) : (
          <FdScrollTable ariaLabel="Succession offers">
            <table className="fd-table">
              <thead>
                <tr>
                  <th scope="col">From</th>
                  <th scope="col">Offered</th>
                  <th scope="col">Expires</th>
                  <th scope="col">Status</th>
                  <th scope="col">Accepted by</th>
                </tr>
              </thead>
              <tbody>
                {detail.transfers.map((t) => (
                  <tr key={t.id}>
                    <td>
                      <FdPersonaChip actor={chip(t.from)} />
                      {t.note ? (
                        <span className="fd-table__sub">{t.note}</span>
                      ) : null}
                    </td>
                    <td className="fd-table__mono">
                      <FdTime iso={t.createdAt} title={t.createdAt}>
                        {stampUTC(t.createdAt)}
                      </FdTime>
                    </td>
                    <td className="fd-table__mono">
                      <FdTime iso={t.expiresAt} title={t.expiresAt}>
                        {stampUTC(t.expiresAt)}
                      </FdTime>
                    </td>
                    <td>
                      <span
                        className="fd-chip"
                        title={
                          t.effectiveStatus === "expired"
                            ? "read from the stored offer being past its expiry"
                            : "the stored status of the offer"
                        }
                      >
                        {t.effectiveStatus}
                      </span>
                    </td>
                    <td>
                      {t.acceptedBy ? (
                        <>
                          <FdPersonaChip actor={chip(t.acceptedBy)} />
                          {t.acceptedAt ? (
                            <span className="fd-table__sub">
                              <FdTime iso={t.acceptedAt} title={t.acceptedAt}>
                                {stampUTC(t.acceptedAt)}
                              </FdTime>
                            </span>
                          ) : null}
                        </>
                      ) : (
                        <span className="fd-note-inline">nobody yet</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </FdScrollTable>
        )}
      </FdSection>
    </>
  );
}

export default function FdContinuityPage({
  scenes,
  selected,
  selectedMissing,
  detail,
  onDownload,
}: FdContinuityPageProps) {
  const totals = scenes.reduce(
    (acc, s) => ({
      changelog: acc.changelog + s.counts.changelog,
      stewards: acc.stewards + s.counts.stewards,
    }),
    { changelog: 0, stewards: 0 },
  );

  return (
    <div className="fd-page fd-stack fd-continuity">
      <FdPageHead title="The standing record" />

      <div className="fd-statrow">
        <FdStat label="Scenes on record" value={groupDigits(scenes.length)} mono />
        <FdStat
          label="Changelog entries"
          value={groupDigits(totals.changelog)}
          mono
        />
        <FdStat
          label="Active stewards"
          value={groupDigits(totals.stewards)}
          mono
        />
      </div>

      <FdSection
        title="The record, scene by scene"
        badge={
          scenes.length > 0 ? (
            <span className="fd-chip">{plural(scenes.length, "scene")}</span>
          ) : undefined
        }
      >
        {scenes.length === 0 ? (
          <p className="fd-empty">No scenes imported yet.</p>
        ) : (
          <div className="fd-board">
            {scenes.map((s) => (
              <article
                className={
                  "fd-card fd-continuity__scene" +
                  (selected === s.id ? " is-selected" : "")
                }
                key={s.id}
              >
                <h3 className="fd-card__title">
                  <a className="fd-cardlink" href={sceneHref(s.id)}>
                    {humanizeEntityTitle(s.title)}
                  </a>
                </h3>
                <p className="fd-chiprow">
                  {s.worldName ? (
                    <span className="fd-chip fd-chip--mono" title={s.sourceNote}>
                      {s.worldName}
                    </span>
                  ) : (
                    <span className="fd-note-inline">not deployed to a World</span>
                  )}
                  <CountChip n={s.counts.changelog} noun="changelog entry" />
                  <CountChip n={s.counts.reports} noun="run" title={RUNS_TITLE} />
                  <CountChip
                    n={s.counts.episodes}
                    noun="run log"
                    title={LOGS_TITLE}
                  />
                  <CountChip n={s.counts.docs} noun="design doc" />
                  <CountChip n={s.counts.stewards} noun="steward" />
                </p>
                <dl className="fd-facts">
                  <div>
                    <dt>Deployed</dt>
                    <dd>
                      {s.deployedAt ? (
                        <FdTime iso={s.deployedAt} title={s.sourceNote}>
                          {dayUTC(s.deployedAt)}
                        </FdTime>
                      ) : (
                        <span className="fd-note-inline">not recorded</span>
                      )}
                    </dd>
                  </div>
                  <div>
                    <dt>Size</dt>
                    <dd>
                      {formatSize(s.sizeBytes) ?? (
                        <span className="fd-note-inline">not recorded</span>
                      )}
                    </dd>
                  </div>
                  <div>
                    <dt>Parcels</dt>
                    <dd>
                      {s.parcels ?? (
                        <span className="fd-note-inline">not recorded</span>
                      )}
                    </dd>
                  </div>
                </dl>
                <p className="fd-card__foot fd-continuity__actions">
                  <Button
                    as="a"
                    variant="secondary"
                    size="sm"
                    href={s.exportHref}
                    download
                    onClick={() => onDownload?.(s.id)}
                  >
                    Download the bundle
                  </Button>
                  <a href={`${s.exportHref}?view`}>View it in the browser</a>
                </p>
              </article>
            ))}
          </div>
        )}
      </FdSection>

      {selectedMissing ? (
        <p className="fd-empty">No stored scene carries that id.</p>
      ) : null}

      {detail ? <SceneDetail detail={detail} onDownload={onDownload} /> : null}
    </div>
  );
}
