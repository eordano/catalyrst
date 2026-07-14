import { useState } from "react";

import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import FdMarkdown from "../components/FdMarkdown";
import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import { dayUTC, plural, stampUTC } from "../fmt";
import {
  FdMarkers,
  machineChipTitle,
  machineDrafted,
  type FdGddHonestyTotals,
  type FdGddSource,
} from "./FdGddListPage";
import "../../atoms/button.css";
import "../components/fdcellchip.css";
import "./fdgdd.css";

export type FdGddSectionVM = {
  name: string;
  open: number;
  tbd: number;
  hypothesis: number;
  agentDecided: number;
};

export type FdGddHypothesisVM = {
  id: string;
  stage: string;
  slug: string;
  status: string;
  ifThen?: string;
  test?: string;
  testedOn?: string;
};

export type FdGddDocVM = {
  id: string;
  title: string;
  kind: string;
  version: number;
  sceneId: string | null;
  supersedes: string | null;
  source: FdGddSource;
  sourceRef: string | null;
  bodyMd: string;
  honesty: { sections: readonly FdGddSectionVM[]; totals: FdGddHonestyTotals };
  hypotheses: readonly FdGddHypothesisVM[];
  createdAt: string;
};

export type FdGddSectionContentVM = {
  name: string;
  contentMd: string;
};

export type FdGddEditVM = {
  /** Saves one section; the server splices it into the body and mints v(n+1). */
  onSaveSection: (index: number, name: string, contentMd: string) => void;
  /** Saves the whole body as v(n+1). */
  onSaveDoc: (bodyMd: string) => void;
  pending: boolean;
  error?: string | null;
};

/** One version in the doc's supersedes chain — stored stats only, never
 *  recomputed. */
export type FdGddVersionVM = {
  id: string;
  version: number;
  createdAt: string;
  open: number;
  /** Stored signature count on this exact version; 0 = nobody signed it. */
  approvals?: number;
};

/** Exactly one tier renders: the truly-linked game, this program's dated
 *  same-concept reading, or the claim-a-seat invitation. */
export type FdGddPlayVM =
  | {
      tier: "play";
      sceneId: string;
      sceneTitle: string;
      runCount: number;
      /** False = the build exists in a repo but no World serves it yet — the
       *  affordance then says what a click actually delivers. */
      deployed?: boolean;
    }
  | {
      tier: "same-concept";
      sceneId: string;
      sceneTitle: string;
      readAt: string;
      rationale: string;
      confidence: string;
      /** Set when the reading named an older version of this chain — the
       *  judgment predates the edits that minted this one. */
      readAgainstVersion?: number | null;
    }
  | {
      tier: "take-on";
      /** Carried into the exchange so the ask can name this design. */
      docTitle: string;
      /** When the doc grounds on an ask, that ask IS the demand — link it. */
      groundedAskId?: string | null;
    };

/** The signature state of THIS version: who signed (resolved names, dates),
 *  whether the viewer already signed, and whether they hold the persona a
 *  signature needs. All display values are stored facts; the empty state is
 *  the honest "no person has approved this version". */
export type FdGddApprovalVM = {
  approvals: readonly { name: string; at: string }[];
  viewerApproved: boolean;
  viewerHasPersona: boolean;
  pending: boolean;
  error?: string | null;
  onApprove: () => void;
};

export type FdGddDocPageProps = {
  doc: FdGddDocVM;
  /** Per-section content, split by the same headings the marker grid counts
   *  by — arrays align index-for-index with doc.honesty.sections. */
  sections?: readonly FdGddSectionContentVM[];
  backHref: string;
  /** Every version in the supersedes chain, version-ordered; the rail. */
  chain?: readonly FdGddVersionVM[];
  /** Sections this version changed vs its predecessor — derived from the two
   *  stored bodies on read. Null = no predecessor; [] = bodies identical
   *  outside the frontmatter (a real reading, shown as such). */
  changed?: readonly string[] | null;
  play?: FdGddPlayVM | null;
  /** Asks this doc's stored grounding keys name — the demand it exists to
   *  answer, resolved to titles server-side. Empty = grounded on nothing. */
  groundedAsks?: readonly { id: string; title: string }[];
  /** Absent (null) hides the whole approval block — a caller that has no
   *  stored answer must not render an invented one. */
  approval?: FdGddApprovalVM | null;
  /** For session-source docs: the last editor's persona name, or null when
   *  the editor never claimed one (renders "a visitor" — never invented). */
  editedBy?: string | null;
  onVersionOpen?: (id: string, version: number) => void;
  onPlayOpen?: (tier: FdGddPlayVM["tier"]) => void;
  /** Absent on superseded docs — only the head of a version chain is editable. */
  edit?: FdGddEditVM | null;
};

/** Everything the document says before its first `## ` heading: the sections
 *  array starts there, so this text has no other home. */
function preambleOf(bodyMd: string): string {
  const lines = bodyMd.split("\n");
  const first = lines.findIndex((l) => /^##\s+\S/.test(l));
  return (first === -1 ? lines : lines.slice(0, first)).join("\n").trim();
}

function GddEditor({
  initial,
  label,
  nextVersion,
  currentVersion,
  pending,
  error,
  onSave,
  onCancel,
}: {
  initial: string;
  label: string;
  nextVersion: number;
  currentVersion: number;
  pending: boolean;
  error?: string | null;
  onSave: (text: string) => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState(initial);
  const rows = Math.min(24, Math.max(6, initial.split("\n").length + 2));
  return (
    <div className="fd-gdd__editor">
      <textarea
        className="fd-form__textarea fd-gdd__editarea"
        aria-label={label}
        value={text}
        rows={rows}
        disabled={pending}
        onChange={(e) => setText(e.target.value)}
      />
      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}
      <div className="fd-form__actions">
        <Button
          type="button"
          variant="primary"
          size="sm"
          disabled={pending}
          onClick={() => onSave(text)}
        >
          Save as v{nextVersion}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={pending}
          onClick={onCancel}
        >
          Cancel
        </Button>
        {pending ? <Spinner size={16} /> : null}
        <span className="fd-note-inline">
          Saving mints v{nextVersion}; v{currentVersion} stays as written.
        </span>
      </div>
    </div>
  );
}

type EditTarget = { kind: "section"; index: number } | { kind: "doc" };

function people(n: number): string {
  return `${n} ${n === 1 ? "person" : "people"}`;
}

function GddVersionRail({
  doc,
  chain,
  onVersionOpen,
}: {
  doc: FdGddDocVM;
  chain: readonly FdGddVersionVM[];
  onVersionOpen?: (id: string, version: number) => void;
}) {
  const maxOpen = Math.max(1, ...chain.map((c) => c.open));
  return (
    <ol className="fd-gdd__versionlist" aria-label="Versions">
      {chain.map((v, i) => {
        const prev = chain[i - 1];
        const delta =
          prev && prev.open !== v.open ? `open ${prev.open}→${v.open}` : null;
        const body = (
          <>
            <span className="fd-gdd__vlabel">v{v.version}</span>
            <FdTime iso={v.createdAt} className="fd-mono" title={stampUTC(v.createdAt)}>
              {dayUTC(v.createdAt)}
            </FdTime>
            <span
              className="fd-gdd__vbar"
              title={`${plural(v.open, "section")} still open`}
              aria-hidden="true"
            >
              <span
                className={"fd-gdd__vfill" + (v.open === 0 ? " is-zero" : "")}
                style={
                  v.open === 0
                    ? undefined
                    : { width: `${Math.round((v.open / maxOpen) * 100)}%` }
                }
              />
            </span>
            <span className="u-sr-only">{plural(v.open, "section")} still open</span>
            {delta ? <span className="fd-gdd__vdelta fd-num">{delta}</span> : null}
            {(v.approvals ?? 0) > 0 ? (
              <span
                className="fd-gdd__vsigned"
                title={`approved by ${people(v.approvals ?? 0)}`}
              >
                ✓ approved
                <span className="u-sr-only"> by {people(v.approvals ?? 0)}</span>
              </span>
            ) : null}
          </>
        );
        return (
          <li key={v.id}>
            {v.id === doc.id ? (
              <span className="fd-gdd__vcard is-current" aria-current="page">
                {body}
              </span>
            ) : (
              <a
                className="fd-gdd__vcard"
                href={`/foundry/gdd/${v.id}`}
                onClick={() => onVersionOpen?.(v.id, v.version)}
              >
                {body}
              </a>
            )}
          </li>
        );
      })}
    </ol>
  );
}

function GddPlayAffordance({
  play,
  onPlayOpen,
}: {
  play: FdGddPlayVM;
  onPlayOpen?: (tier: FdGddPlayVM["tier"]) => void;
}) {
  if (play.tier === "play") {
    return (
      <>
        <Button
          as="a"
          variant="primary"
          size="sm"
          href={`/foundry/play/${play.sceneId}`}
          onClick={() => onPlayOpen?.("play")}
        >
          {play.deployed === false
            ? `See the build — ${play.sceneTitle}`
            : `Play ${play.sceneTitle}`}
        </Button>
        {play.runCount > 0 ? (
          <a
            className="fd-chip"
            href={`/foundry/console/trajectories?scene=${encodeURIComponent(play.sceneId)}`}
          >
            {plural(play.runCount, "run")} recorded
          </a>
        ) : null}
      </>
    );
  }
  if (play.tier === "same-concept") {
    // A dashed chip, never a button: adjacency this program read on a date,
    // not the game this document describes.
    const readAgainst =
      play.readAgainstVersion != null ? `, read against v${play.readAgainstVersion}` : "";
    const reading = `this program's reading, ${dayUTC(play.readAt)}${readAgainst} — ${play.rationale} (${play.confidence})`;
    return (
      <a
        className="fd-cellchip fd-cellchip--link fd-gdd__adjacent"
        href={`/foundry/play/${play.sceneId}`}
        title={reading}
        onClick={() => onPlayOpen?.("same-concept")}
      >
        Same concept, live: {play.sceneTitle}
        <span className="u-sr-only"> — {reading}</span>
      </a>
    );
  }
  return (
    <Button
      as="a"
      variant="ghost"
      size="sm"
      href={
        play.groundedAskId
          ? `/foundry/exchange/${play.groundedAskId}`
          : `/foundry/exchange?ask=${encodeURIComponent(play.docTitle)}`
      }
      onClick={() => onPlayOpen?.("take-on")}
    >
      Take this design on
    </Button>
  );
}

export default function FdGddDocPage({
  doc,
  sections = [],
  backHref,
  chain = [],
  changed = null,
  play = null,
  groundedAsks = [],
  approval = null,
  editedBy = null,
  onVersionOpen,
  onPlayOpen,
  edit = null,
}: FdGddDocPageProps) {
  const [editing, setEditing] = useState<EditTarget | null>(null);
  const nextVersion = doc.version + 1;
  const preamble = preambleOf(doc.bodyMd);
  const head = chain.length > 0 ? chain[chain.length - 1] : undefined;
  const successor = head && head.version > doc.version ? head : null;

  return (
    <div className="fd-page fd-stack fd-gdd">
      <FdPageHead
        eyebrow="Design docs"
        title={doc.title}
        crumbs={<a href={backHref}>← All design docs</a>}
        aside={play ? <GddPlayAffordance play={play} onPlayOpen={onPlayOpen} /> : null}
      />

      {changed !== null ? (
        <p className="fd-note">
          {changed.length === 0
            ? `No section text changed from v${doc.version - 1}.`
            : `Changed from v${doc.version - 1}: ${changed
                .map((c) => `“${c}”`)
                .join(" · ")}`}
        </p>
      ) : null}

      {groundedAsks.map((a) => (
        <p className="fd-note" key={a.id}>
          Grounded on the ask{" "}
          <a href={`/foundry/exchange/${a.id}`}>“{a.title}” →</a>
        </p>
      ))}

      {successor ? (
        <p className="fd-gdd__superseded">
          This is v{doc.version}, superseded —{" "}
          <a
            href={`/foundry/gdd/${successor.id}`}
            onClick={() => onVersionOpen?.(successor.id, successor.version)}
          >
            v{successor.version} of {dayUTC(successor.createdAt)} is current
          </a>
          .
        </p>
      ) : null}

      <FdSection title="This design doc">
        <dl className="fd-facts">
          <div>
            <dt>Kind</dt>
            <dd className="fd-mono">{doc.kind}</dd>
          </div>
          <div>
            <dt>Version</dt>
            <dd>v{doc.version}</dd>
          </div>
          <div>
            <dt>Written</dt>
            <dd>
              <FdTime iso={doc.createdAt} title={stampUTC(doc.createdAt)}>
                {dayUTC(doc.createdAt)}
              </FdTime>
            </dd>
          </div>
          <div>
            <dt>Source</dt>
            <dd className="fd-chiprow">
              {machineDrafted(doc.source) ? (
                <span className="fd-chip" title={machineChipTitle(doc.createdAt)}>
                  drafted by this program
                  <span className="u-sr-only"> — {machineChipTitle(doc.createdAt)}</span>
                </span>
              ) : null}
              {doc.source === "session" ? (
                <span
                  className="fd-chip"
                  title={`text saved by a visitor on this site, ${dayUTC(doc.createdAt)}`}
                >
                  edited by {editedBy ?? "a visitor"}
                  <span className="u-sr-only">
                    {" "}
                    — text saved by a visitor on this site, {dayUTC(doc.createdAt)}
                  </span>
                </span>
              ) : null}
              {doc.sourceRef ? (
                doc.source === "slack-import" ? (
                  // New tab on purpose: the doc keeps its place here.
                  <a
                    className="fd-chip"
                    href={doc.sourceRef}
                    target="_blank"
                    rel="noreferrer"
                    title="workspace members only"
                  >
                    Slack thread
                    <span className="u-sr-only"> — workspace members only</span>
                  </a>
                ) : (
                  <span className="fd-mono fd-gdd__ref">{doc.sourceRef}</span>
                )
              ) : machineDrafted(doc.source) || doc.source === "session" ? null : (
                <span className="fd-note-inline">not recorded</span>
              )}
            </dd>
          </div>
          <div>
            <dt>Markers</dt>
            <dd>
              <FdMarkers
                totals={doc.honesty.totals}
                label="Marker totals"
                empty={<span className="fd-note-inline">none</span>}
              />
            </dd>
          </div>
          {approval ? (
            <div>
              <dt>Approved</dt>
              <dd className="fd-chiprow">
                {approval.approvals.length === 0 ? (
                  <span className="fd-note-inline">
                    no person has approved this version
                  </span>
                ) : (
                  approval.approvals.map((a) => (
                    <span
                      className="fd-chip"
                      key={`${a.name}-${a.at}`}
                      title={stampUTC(a.at)}
                    >
                      by {a.name}, <FdTime iso={a.at}>{dayUTC(a.at)}</FdTime>
                    </span>
                  ))
                )}
              </dd>
            </div>
          ) : null}
        </dl>
        {approval ? (
          <div className="fd-gdd__approve">
            {approval.viewerApproved ? (
              <p className="fd-note">You approved this version.</p>
            ) : approval.viewerHasPersona ? (
              <Button
                variant="secondary"
                size="sm"
                onClick={approval.onApprove}
                disabled={approval.pending}
              >
                Approve v{doc.version}
              </Button>
            ) : (
              <p className="fd-note">
                An approval is signed with a name —{" "}
                <a href="/foundry/persona">claim a persona first →</a>
              </p>
            )}
            {approval.error ? (
              <p className="fd-alert" role="alert">
                {approval.error}
              </p>
            ) : null}
          </div>
        ) : null}
      </FdSection>

      <FdSection
        title="The document"
        badge={
          doc.honesty.sections.length > 0 ? (
            <span className="fd-chip fd-num">{doc.honesty.sections.length}</span>
          ) : null
        }
        aside={
          edit && editing === null ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={edit.pending}
              onClick={() => setEditing({ kind: "doc" })}
            >
              Edit the full document
            </Button>
          ) : null
        }
      >
        {editing?.kind === "doc" && edit ? (
          <GddEditor
            initial={doc.bodyMd}
            label="Edit the full document"
            nextVersion={nextVersion}
            currentVersion={doc.version}
            pending={edit.pending}
            error={edit.error}
            onSave={(text) => edit.onSaveDoc(text)}
            onCancel={() => setEditing(null)}
          />
        ) : (
          <>
            {preamble ? (
              <div className="fd-gdd__preamble">
                <FdMarkdown source={preamble} />
              </div>
            ) : null}
            {/* Each section keeps the markers counted inside it, beside its own
                heading, and can be folded away. */}
            <div className="fd-gdd__sections">
              {doc.honesty.sections.map((s, i) => {
                const content = sections[i];
                return (
                  <details key={`${s.name}-${i}`} className="fd-gdd__section" open>
                    <summary className="fd-gdd__sectionrow">
                      <h3 className="fd-gdd__sectionname">{s.name}</h3>
                      <FdMarkers totals={s} label={`Markers in ${s.name}`} />
                    </summary>
                    <div className="fd-gdd__sectionbody">
                      {content === undefined || content.name !== s.name ? (
                        <p className="fd-empty">This section&apos;s text is not stored.</p>
                      ) : editing?.kind === "section" && editing.index === i && edit ? (
                        <GddEditor
                          initial={content.contentMd}
                          label={`Edit section ${s.name}`}
                          nextVersion={nextVersion}
                          currentVersion={doc.version}
                          pending={edit.pending}
                          error={edit.error}
                          onSave={(text) => edit.onSaveSection(i, content.name, text)}
                          onCancel={() => setEditing(null)}
                        />
                      ) : (
                        <>
                          <FdMarkdown source={content.contentMd} />
                          {edit && editing === null ? (
                            <div className="fd-gdd__sectionfoot">
                              <Button
                                type="button"
                                variant="ghost"
                                size="sm"
                                disabled={edit.pending}
                                onClick={() => setEditing({ kind: "section", index: i })}
                              >
                                Edit this section
                              </Button>
                            </div>
                          ) : null}
                        </>
                      )}
                    </div>
                  </details>
                );
              })}
            </div>
          </>
        )}
      </FdSection>

      {/* An empty log is header noise, not honesty — the section only earns
          its place when experiment files actually shipped with the doc. */}
      {doc.hypotheses.length === 0 ? null : (
        <FdSection
          title="Hypothesis log"
          badge={<span className="fd-chip fd-num">{doc.hypotheses.length}</span>}
        >
          <FdScrollTable ariaLabel="Hypothesis log">
            <table className="fd-table">
              <thead>
                <tr>
                  <th scope="col">ID</th>
                  <th scope="col">Stage / section</th>
                  <th scope="col">Status</th>
                  <th scope="col">IF / THEN</th>
                  <th scope="col">Cheapest killing test</th>
                  <th scope="col">Tested on</th>
                </tr>
              </thead>
              <tbody>
                {doc.hypotheses.map((h) => (
                  <tr key={h.id}>
                    <th scope="row" className="fd-mono">
                      {h.id}
                      <span className="fd-table__sub">{`${h.id}-${h.slug}_${h.status}.md`}</span>
                    </th>
                    <td
                      className={
                        h.stage.startsWith("section ") || h.stage === "appendix"
                          ? "fd-table__prose"
                          : "fd-mono"
                      }
                    >
                      {h.stage}
                    </td>
                    <td>
                      <span
                        className={`fd-chip fd-hyp--${h.status}`}
                        title="read from the experiment file's own name"
                      >
                        {h.status}
                      </span>
                    </td>
                    <td className="fd-table__prose">{h.ifThen ?? "—"}</td>
                    <td className="fd-table__prose">{h.test ?? "—"}</td>
                    <td>{h.testedOn ?? "not yet"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </FdScrollTable>
        </FdSection>
      )}

      {chain.length > 1 ? (
        <FdSection
          title="Versions"
          badge={<span className="fd-chip fd-num">{chain.length}</span>}
        >
          <GddVersionRail doc={doc} chain={chain} onVersionOpen={onVersionOpen} />
        </FdSection>
      ) : null}
    </div>
  );
}
