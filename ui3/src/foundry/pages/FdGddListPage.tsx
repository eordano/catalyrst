import EmptyState from "../../components/EmptyState";
import FdProvenancePill from "../components/FdProvenancePill";
import FdSection, { FdPageHead } from "../components/FdSection";
import "./fdgdd.css";

export type FdGddHonestyTotals = {
  open: number;
  tbd: number;
  hypothesis: number;
  agentDecided: number;
};

export type FdGddListRowVM = {
  id: string;
  title: string;
  kind: string;
  version: number;
  /** The doc this one replaces, if any. */
  supersedes?: string | null;
  sceneId: string | null;
  sceneTitle: string | null;
  source: "slack-import" | "copilot";
  sourceRef: string | null;
  honesty: FdGddHonestyTotals;
  hypothesisCounts: Readonly<Record<string, number>>;
  createdAt: string;
};

export const MARKER_LABELS: readonly { key: keyof FdGddHonestyTotals; label: string }[] = [
  { key: "open", label: "[OPEN]" },
  { key: "tbd", label: "TBD:" },
  { key: "hypothesis", label: "[HYPOTHESIS]" },
  { key: "agentDecided", label: "[agent-decided]" },
];

export function gddDay(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toISOString().slice(0, 10);
}

export type FdGddListPageProps = {
  docs: readonly FdGddListRowVM[];
};

export default function FdGddListPage({ docs }: FdGddListPageProps) {
  // A doc named as another doc's `supersedes` target is the older revision. Map
  // it to the version that replaced it so the list can say which is current,
  // instead of showing two identically-titled docs with no way to tell them apart.
  const supersededBy = new Map<string, number>();
  for (const d of docs) {
    if (d.supersedes) supersededBy.set(d.supersedes, d.version);
  }

  return (
    <div className="fd-page fd-stack fd-gdd">
      <FdPageHead
        title="Design docs"
        intro="shortGDDs in the Creator Success pre-production format. Open questions stay marked TBD instead of being papered over — the counts below are the documents' own markers, tallied section by section at import."
      />

      <FdSection title="Documents">
        {docs.length === 0 ? (
          <EmptyState
            variant="inline"
            title="No design docs yet."
            subtitle="Draft one through the copilot's /gdd command, or import an existing shortGDD."
          />
        ) : (
          <ul className="fd-gdd__list">
            {docs.map((doc) => {
              const hyp = Object.entries(doc.hypothesisCounts).filter(([, n]) => n > 0);
              return (
                <li key={doc.id} className="fd-gddcard">
                  <div className="fd-gddcard__head">
                    <h3 className="fd-gddcard__title">
                      <a href={`/foundry/gdd/${doc.id}`}>{doc.title}</a>
                    </h3>
                    <span className="fd-chip">v{doc.version}</span>
                    <span className="fd-chip fd-chip--mono">{doc.kind}</span>
                    {supersededBy.has(doc.id) ? (
                      <span className="fd-chip">
                        superseded by v{supersededBy.get(doc.id)}
                      </span>
                    ) : null}
                    <FdProvenancePill
                      provenance={doc.source === "copilot" ? "recorded" : "imported"}
                    />
                  </div>

                  <p className="fd-gddcard__markers">
                    {MARKER_LABELS.map((m) => (
                      <span
                        key={m.key}
                        className={
                          "fd-gddcard__marker" +
                          (doc.honesty[m.key] === 0 ? " is-zero" : "")
                        }
                      >
                        <span className="fd-mono">{m.label}</span> {doc.honesty[m.key]}
                      </span>
                    ))}
                  </p>

                  <p className="fd-gddcard__meta">
                    {hyp.length === 0
                      ? "No hypothesis log filed"
                      : hyp.map(([status, n]) => `${n} ${status}`).join(" · ")}
                    {doc.sceneTitle ? (
                      <>
                        {" · "}
                        <a href={`/foundry/play/${doc.sceneId}`}>{doc.sceneTitle}</a>
                      </>
                    ) : null}
                    {" · "}
                    {gddDay(doc.createdAt)}
                    {doc.sourceRef ? (
                      <>
                        {" · "}
                        <a href={doc.sourceRef} rel="noreferrer">
                          {doc.source === "copilot" ? "copilot workspace" : "Slack thread"}
                        </a>
                      </>
                    ) : null}
                  </p>
                </li>
              );
            })}
          </ul>
        )}
      </FdSection>

    </div>
  );
}
