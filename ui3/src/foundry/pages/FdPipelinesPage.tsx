import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import { plural, stampUTC } from "../fmt";
import "./fdpipelines.css";

export type FdPipelineRowVM = {
  slug: string;
  title: string;
  passed: number;
  total: number;
  /** First non-passed step id; null when every step passed. */
  next: string | null;
  created: string;
  href: string;
};

export type FdPipelinesPageProps = {
  pipelines: readonly FdPipelineRowVM[];
};

export default function FdPipelinesPage({ pipelines }: FdPipelinesPageProps) {
  return (
    <div className="fd-stack fd-pipelines">
      <FdPageHead
        eyebrow="Console"
        title="Pipelines"
        intro="Staged design runs from the copilot — each step gated before the next can start."
      />

      <FdSection
        title="Every pipeline"
        badge={
          pipelines.length > 0 ? (
            <span className="fd-chip">{plural(pipelines.length, "pipeline")}</span>
          ) : undefined
        }
      >
        {pipelines.length === 0 ? (
          <p className="fd-empty">No pipelines yet.</p>
        ) : (
          <FdScrollTable ariaLabel="Pipelines">
            <table className="fd-table">
              <thead>
                <tr>
                  <th scope="col">Pipeline</th>
                  <th scope="col">Title</th>
                  <th scope="col">Steps</th>
                  <th scope="col">Next</th>
                  <th scope="col">Started</th>
                </tr>
              </thead>
              <tbody>
                {pipelines.map((p) => (
                  <tr key={p.slug}>
                    <th scope="row" className="fd-mono">
                      <a href={p.href}>{p.slug}</a>
                    </th>
                    <td className="fd-table__prose">{p.title}</td>
                    <td>{`${p.passed} of ${plural(p.total, "step")} passed`}</td>
                    <td className="fd-mono">{p.next ?? "complete"}</td>
                    <td>
                      <FdTime iso={p.created} className="fd-mono">
                        {stampUTC(p.created)}
                      </FdTime>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </FdScrollTable>
        )}
      </FdSection>
    </div>
  );
}
