import FdMarkdown from "../components/FdMarkdown";
import FdSection, { FdPageHead } from "../components/FdSection";
import { stampUTC } from "../fmt";
import "./fdpipeline.css";

export type FdPipelineStepVM = {
  id: string;
  /** Stored status word: pending | passed | failed. */
  status: string;
  /** Workspace-relative artifact path, as stored. */
  artifact: string | null;
  /** Gate problems, verbatim from pipeline.json. */
  problems: readonly string[];
  updated: string | null;
  /** Artifact file text; null when artifact is null or the file is unreadable. */
  content: string | null;
};

export type FdPipelinePageProps = {
  slug: string;
  title: string;
  kind: string;
  created: string;
  steps: readonly FdPipelineStepVM[];
};

function StepChip({ step }: { step: FdPipelineStepVM }) {
  const word = step.status === "pending" && step.updated === null ? "not run" : step.status;
  const stamp = step.updated ? `pipeline.json · ${stampUTC(step.updated)}` : undefined;
  return (
    <span className="fd-chip" title={stamp}>
      {word}
      {stamp ? <span className="u-sr-only"> — {stamp}</span> : null}
    </span>
  );
}

function StepBody({ step }: { step: FdPipelineStepVM }) {
  if (step.status === "failed") {
    return (
      <ul className="fd-pipeline__problems">
        {step.problems.map((problem, i) => (
          <li key={i}>{problem}</li>
        ))}
      </ul>
    );
  }
  if (step.status === "passed") {
    return (
      <>
        {step.artifact ? (
          <p className="fd-pipeline__artifact fd-mono">{step.artifact}</p>
        ) : null}
        {step.content !== null ? (
          <div className="fd-pipeline__doc">
            <FdMarkdown source={step.content} />
          </div>
        ) : (
          <p className="fd-empty">Artifact file missing.</p>
        )}
      </>
    );
  }
  return step.updated === null ? (
    <p className="fd-empty">This step has not run.</p>
  ) : null;
}

export default function FdPipelinePage({
  slug,
  title,
  created,
  steps,
}: FdPipelinePageProps) {
  return (
    <div className="fd-stack fd-pipeline">
      <FdPageHead
        eyebrow="Pipeline"
        title={title}
        intro={`${slug} — started ${stampUTC(created)}`}
        crumbs={<a href="/foundry/console/pipelines">← All pipelines</a>}
      />

      {steps.map((step, i) => (
        <FdSection
          key={step.id}
          title={`${i + 1}. ${step.id}`}
          badge={<StepChip step={step} />}
        >
          <StepBody step={step} />
        </FdSection>
      ))}
    </div>
  );
}
