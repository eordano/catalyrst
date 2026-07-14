import FdProvenancePill from "../components/FdProvenancePill";
import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FdPageHead } from "../components/FdSection";
import { MARKER_LABELS, type FdGddHonestyTotals } from "./FdGddListPage";
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
  source: "slack-import" | "copilot";
  sourceRef: string | null;
  bodyMd: string;
  honesty: { sections: readonly FdGddSectionVM[]; totals: FdGddHonestyTotals };
  hypotheses: readonly FdGddHypothesisVM[];
  createdAt: string;
};

export type FdGddDocPageProps = {
  doc: FdGddDocVM;
  backHref: string;
};

export default function FdGddDocPage({ doc, backHref }: FdGddDocPageProps) {
  const openSections = doc.honesty.sections.filter((s) => s.open > 0);

  return (
    <div className="fd-page fd-stack fd-gdd">
      <FdPageHead
        eyebrow="Design doc"
        title={doc.title}
        intro={
          openSections.length === 0
            ? "Every section of this document has been answered. The markers below are what the author left standing as unknowns and untested claims — they are not defects, they are the document being honest."
            : `${openSections.length} of ${doc.honesty.sections.length} sections are still marked [OPEN]. This is a draft in progress and says so.`
        }
        aside={
          <span className="fd-gdd__chips">
            <span className="fd-chip">v{doc.version}</span>
            <span className="fd-chip fd-chip--mono">{doc.kind}</span>
            <FdProvenancePill
              provenance={doc.source === "copilot" ? "recorded" : "imported"}
            />
          </span>
        }
      />

      <p className="fd-gdd__crumbs">
        <a href={backHref}>← All design docs</a>
        {doc.supersedes ? (
          <>
            {" · supersedes "}
            <a href={`/foundry/gdd/${doc.supersedes}`}>{doc.supersedes}</a>
          </>
        ) : null}
        {doc.sceneId ? (
          <>
            {" · "}
            <a href={`/foundry/play/${doc.sceneId}`}>the game</a>
          </>
        ) : null}
        {doc.sourceRef ? (
          <>
            {" · "}
            <a href={doc.sourceRef} rel="noreferrer">
              {doc.source === "copilot" ? "copilot workspace" : "Slack thread"}
            </a>
          </>
        ) : null}
      </p>

      <FdSection title="Honesty markers">
        <p className="fd-gdd__totals">
          {MARKER_LABELS.map((m) => (
            <span key={m.key} className="fd-gdd__total">
              <span className="fd-mono">{m.label}</span>
              <strong>{doc.honesty.totals[m.key]}</strong>
            </span>
          ))}
        </p>

        <FdScrollTable ariaLabel="Honesty markers by section">
          <table className="fd-table">
            <thead>
              <tr>
                <th scope="col">Section</th>
                {MARKER_LABELS.map((m) => (
                  <th key={m.key} scope="col" className="fd-mono">
                    {m.label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {doc.honesty.sections.map((s) => (
                <tr key={s.name} className={s.open > 0 ? "is-open" : undefined}>
                  <th scope="row">{s.name}</th>
                  {MARKER_LABELS.map((m) => (
                    <td key={m.key} className="fd-num">
                      {s[m.key] === 0 ? "—" : s[m.key]}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </FdScrollTable>
      </FdSection>

      <FdSection
        title="Hypothesis log"
        sub="Status comes from each experiment file's own name — H<stage>-<nn>-<slug>_<status>.md — and from nothing else."
      >
        {doc.hypotheses.length === 0 ? (
          <p className="fd-note">
            No experiment files shipped with this document, so there is no hypothesis log
            to show. Rows are not manufactured out of the prose.
          </p>
        ) : (
          <FdScrollTable ariaLabel="Hypothesis log">
            <table className="fd-table">
              <thead>
                <tr>
                  <th scope="col">ID</th>
                  <th scope="col">Stage</th>
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
                    </th>
                    <td className="fd-mono">{h.stage}</td>
                    <td>
                      <span className={`fd-hyp fd-hyp--${h.status}`}>{h.status}</span>
                    </td>
                    <td className="fd-gdd__claim">{h.ifThen ?? "—"}</td>
                    <td className="fd-gdd__claim">{h.test ?? "—"}</td>
                    <td>{h.testedOn ?? "not yet"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </FdScrollTable>
        )}
      </FdSection>

      <FdSection title="The document">
        <p className="fd-note">
          Shown exactly as it was written — no rendering pass, no reflow, no summary.
        </p>
        <pre className="fd-gdd__body">{doc.bodyMd}</pre>
      </FdSection>

    </div>
  );
}
