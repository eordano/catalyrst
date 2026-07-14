import type { ReactNode } from "react";

import Button from "../../atoms/Button";
import FdProvenancePill from "../components/FdProvenancePill";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import { dayUTC, stampUTC } from "../fmt";
import "./fdgdd.css";

export type FdGddHonestyTotals = {
  open: number;
  tbd: number;
  hypothesis: number;
  agentDecided: number;
};

export type FdGddSource = "slack-import" | "copilot" | "program" | "session";

export type FdGddListRowVM = {
  id: string;
  title: string;
  kind: string;
  version: number;
  /** The doc this one replaces, if any. */
  supersedes?: string | null;
  sceneId: string | null;
  sceneTitle: string | null;
  source: FdGddSource;
  sourceRef: string | null;
  honesty: FdGddHonestyTotals;
  hypothesisCounts: Readonly<Record<string, number>>;
  /** Stored signature count on this exact version; 0 = nobody signed it. */
  approvals?: number;
  createdAt: string;
};

export const MARKER_LABELS: readonly {
  key: keyof FdGddHonestyTotals;
  label: string;
  gloss: string;
}[] = [
  { key: "open", label: "[OPEN]", gloss: "section not yet written" },
  { key: "tbd", label: "TBD:", gloss: "value not known yet" },
  { key: "hypothesis", label: "[HYPOTHESIS]", gloss: "untested claim" },
  { key: "agentDecided", label: "[agent-decided]", gloss: "filled by the drafting agent" },
];

/** Status wording that carries a stored meaning; anything else renders bare. */
const HYPOTHESIS_GLOSS: Record<string, string | undefined> = {
  parked: "set aside, not yet tested",
};

/** True only for verbatim machine text. */
export function machineDrafted(source: FdGddSource): boolean {
  // Verbatim machine text only: copilot transcripts and program-authored
  // briefs. slack-import is human-written; session is a visitor's edit —
  // mixed text a person saved, which must never read "no person wrote this".
  return source === "copilot" || source === "program";
}

export function machineChipTitle(createdAt: string): string {
  return `no person wrote this text — recorded ${dayUTC(createdAt) ?? createdAt}`;
}

/** The four honesty markers, one rendering everywhere. A marker the document
 *  does not carry is an absence, not a zero row. */
export function FdMarkers({
  totals,
  label,
  empty = null,
}: {
  totals: FdGddHonestyTotals;
  label: string;
  empty?: ReactNode;
}) {
  const carried = MARKER_LABELS.filter((m) => totals[m.key] > 0);
  if (carried.length === 0) return <>{empty}</>;
  return (
    <span className="fd-chiprow fd-markers" role="group" aria-label={label}>
      {carried.map((m) => (
        <span key={m.key} className="fd-chip fd-chip--mono" title={m.gloss}>
          {m.label} {totals[m.key]}
          <span className="u-sr-only"> — {m.gloss}</span>
        </span>
      ))}
    </span>
  );
}

export function FdHypothesisChips({
  counts,
}: {
  counts: Readonly<Record<string, number>>;
}) {
  const filed = Object.entries(counts).filter(([, n]) => n > 0);
  if (filed.length === 0) {
    return (
      <span
        className="fd-chip"
        title="a hypothesis is stored by its own experiment file"
      >
        no hypotheses stored
      </span>
    );
  }
  return (
    <>
      {filed.map(([status, n]) => {
        const gloss = HYPOTHESIS_GLOSS[status];
        return (
          <span key={status} className="fd-chip" title={gloss}>
            {n} {status}
            {gloss ? <span className="u-sr-only"> — {gloss}</span> : null}
          </span>
        );
      })}
    </>
  );
}

export type FdGddPublishResultVM = {
  ok: boolean;
  error: string | null;
  id?: string;
  title?: string;
};

export type FdGddListPageProps = {
  docs: readonly FdGddListRowVM[];
  /** Outcome of the last publish-from-session post, if this render follows one. */
  publish?: FdGddPublishResultVM | null;
};

function GddCard({
  doc,
  replacedBy,
}: {
  doc: FdGddListRowVM;
  replacedBy?: number;
}) {
  return (
    <li className="fd-card fd-gddcard">
      <h3 className="fd-card__title">
        <a className="fd-cardlink" href={`/foundry/gdd/${doc.id}`}>
          {doc.title}
        </a>
      </h3>

      <p className="fd-chiprow fd-gddcard__facts">
        <span className="fd-chip">v{doc.version}</span>
        <span className="fd-chip fd-chip--mono">{doc.kind}</span>
        {replacedBy ? (
          <span className="fd-chip">superseded by v{replacedBy}</span>
        ) : null}
        {(doc.approvals ?? 0) > 0 ? (
          <span
            className="fd-chip fd-gddcard__approved"
            title={`${doc.approvals} recorded signature${doc.approvals === 1 ? "" : "s"} on this version`}
          >
            approved
            <span className="u-sr-only">
              {" "}
              — {doc.approvals} recorded signature
              {doc.approvals === 1 ? "" : "s"} on this version
            </span>
          </span>
        ) : null}
        <FdProvenancePill
          provenance={
            doc.source === "slack-import"
              ? "imported"
              : doc.source === "session"
                ? "visitor"
                : "recorded"
          }
        />
        {doc.source === "session" ? (
          <span
            className="fd-chip"
            title="text saved by a visitor on this site"
          >
            edited by a visitor
            <span className="u-sr-only">
              {" "}
              — text saved by a visitor on this site
            </span>
          </span>
        ) : null}
        {machineDrafted(doc.source) ? (
          <span className="fd-chip" title={machineChipTitle(doc.createdAt)}>
            drafted by this program
            <span className="u-sr-only"> — {machineChipTitle(doc.createdAt)}</span>
          </span>
        ) : null}
        {doc.sceneTitle ? (
          <a className="fd-chip" href={`/foundry/play/${doc.sceneId}`}>
            {doc.sceneTitle}
          </a>
        ) : doc.kind === "brief" ? (
          // A brief that proposes no game is not "proposed".
          <span className="fd-chip">open ground — no game named</span>
        ) : (
          <span className="fd-chip">proposed — not built here</span>
        )}
        <FdTime
          iso={doc.createdAt}
          className="fd-chip"
          title={stampUTC(doc.createdAt)}
          sr={stampUTC(doc.createdAt)}
        >
          {dayUTC(doc.createdAt)}
        </FdTime>
        <FdHypothesisChips counts={doc.hypothesisCounts} />
        {doc.sourceRef ? (
          doc.source === "slack-import" ? (
            <a
              className="fd-chip"
              href={doc.sourceRef}
              rel="noreferrer"
              title="workspace members only"
            >
              Slack thread
              <span className="u-sr-only"> — workspace members only</span>
            </a>
          ) : (
            // A copilot or program ref is a repo/workspace path, not a URL —
            // shown as text, never an anchor that would 404.
            <span className="fd-mono fd-gddcard__ref">{doc.sourceRef}</span>
          )
        ) : null}
      </p>

      <FdMarkers totals={doc.honesty} label={`Markers in ${doc.title}`} />
    </li>
  );
}

export default function FdGddListPage({ docs, publish = null }: FdGddListPageProps) {
  // A doc named as another doc's `supersedes` target is the older revision;
  // the two groups below are that fact, shown instead of sorted for.
  const replacedBy = new Map<string, number>();
  for (const d of docs) {
    if (d.supersedes) replacedBy.set(d.supersedes, d.version);
  }
  const current = docs.filter((d) => !replacedBy.has(d.id));
  const replaced = docs.filter((d) => replacedBy.has(d.id));

  return (
    <div className="fd-page fd-stack fd-gdd">
      <FdPageHead title="Design docs" />

      <FdSection
        title="Current"
        badge={<span className="fd-chip fd-num">{current.length}</span>}
      >
        {current.length === 0 ? (
          <p className="fd-empty">No design docs yet.</p>
        ) : (
          <ul className="fd-gdd__list">
            {current.map((doc) => (
              <GddCard key={doc.id} doc={doc} />
            ))}
          </ul>
        )}
      </FdSection>

      {replaced.length > 0 ? (
        <FdSection
          title="Superseded"
          badge={<span className="fd-chip fd-num">{replaced.length}</span>}
        >
          <ul className="fd-gdd__list">
            {replaced.map((doc) => (
              <GddCard key={doc.id} doc={doc} replacedBy={replacedBy.get(doc.id)} />
            ))}
          </ul>
        </FdSection>
      ) : null}

      <FdSection
        title="Publish a draft"
        sub={
          <>
            Draft in <a href="/foundry/copilot">the copilot</a> with /gdd — or
            /brief to start from a stored ask — then paste the session id from
            its address bar.
          </>
        }
      >
        <form className="fd-form fd-gdd__publish" method="post">
          <input type="hidden" name="intent" value="publish" />
          <div className="fd-form__field">
            <label className="fd-form__label" htmlFor="fd-gdd-session">
              Copilot session id
            </label>
            <input
              id="fd-gdd-session"
              className="fd-form__input fd-gdd__publish-input"
              name="sessionId"
              type="text"
              placeholder="ses_…"
              pattern="ses_[A-Za-z0-9]+"
              required
              autoComplete="off"
            />
          </div>
          <div className="fd-form__actions">
            <Button type="submit" variant="primary" size="sm">
              Publish
            </Button>
          </div>
        </form>
        {publish ? (
          publish.ok ? (
            <p className="fd-note" role="status">
              Published <a href={`/foundry/gdd/${publish.id}`}>{publish.title}</a>.
            </p>
          ) : (
            <p className="fd-alert" role="alert">
              {publish.error}
            </p>
          )
        ) : null}
      </FdSection>
    </div>
  );
}
