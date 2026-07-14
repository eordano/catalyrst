import { useEffect, useRef } from "react";
import type React from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GvSubmitProjectUpdate from "@ui/governance/pages/GvSubmitProjectUpdate";
import GvProjectUpdateDetail from "@ui/governance/pages/GvProjectUpdateDetail";
import GvProposalDetailSuccessOutcomeScreens from "@ui/governance/components/GvProposalDetailSuccessOutcomeScreens";

import type { ProjectUpdateContext } from "@data/lib/catalyst/governance/project-update";
import type { TrackContext } from "@core/lib/telemetry/track";
import {
  projectUpdateMachine,
  resolveUpdateSnapshot,
  slugToState,
  stateToSlug,
  type PublishFn,
  type TrackFn,
  type UpdateDraft,
} from "./machine";

export type SubmitProjectUpdateWizardProps = {
  context: ProjectUpdateContext;
  trackCtx: TrackContext;
  initialStep?: string;
  publishUpdate?: PublishFn;
  track?: TrackFn;
};

export default function SubmitProjectUpdateWizard({
  context,
  trackCtx,
  initialStep,
  publishUpdate,
  track,
}: SubmitProjectUpdateWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <SubmitProjectUpdateWizardInner
      key={stateId}
      stateId={stateId}
      context={context}
      trackCtx={trackCtx}
      publishUpdate={publishUpdate}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  context: ProjectUpdateContext;
  trackCtx: TrackContext;
  publishUpdate?: PublishFn;
  track?: TrackFn;
};

function deepLinkDraft(): Partial<UpdateDraft> {
  return {
    health: "onTrack",
    introduction:
      "This update covers the latest reporting period for the project.",
    highlights:
      "Shipped the planned milestones and made measurable progress.",
    blockers: "No blockers this period.",
    next_steps: "Continue with the roadmap; start the next milestone.",
    disclosed: 0,
    records: 0,
  };
}

function SubmitProjectUpdateWizardInner({
  stateId,
  context,
  trackCtx,
  publishUpdate,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const { project, funding, csvHeader } = context;

  const snapshot = useRef(
    resolveUpdateSnapshot({
      step: stateId,
      trackCtx,
      projectId: project.id,
      publishUpdate,
      track,
      draft: deepLinkDraft(),
    }),
  ).current;

  const [state, send] = useMachine(projectUpdateMachine, {
    input: {
      trackCtx,
      projectId: project.id,
      publishUpdate,
      track,
      draft: deepLinkDraft(),
    },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const draft = state.context.draft;

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  const fmtAmount = (n: number) => Math.round(n).toLocaleString("en-US");
  const undisclosedNum = Math.max(
    0,
    funding.releasedSinceLastUpdate - draft.disclosed,
  );
  const funds = {
    released: fmtAmount(funding.releasedSinceLastUpdate),
    disclosed: fmtAmount(draft.disclosed),
    token: funding.token,
    releasedNote:
      funding.txCountSinceLastUpdate > 0
        ? `${funding.txCountSinceLastUpdate} tx.`
        : "No releases recorded this period",
    undisclosedNote: `${fmtAmount(undisclosedNum)} ${funding.token} left undisclosed`,
  };

  const perRecord = draft.records ? Math.round(draft.disclosed / draft.records) : 0;
  const previewUpdate = {
    health: draft.health,
    introduction: draft.introduction,
    highlights: draft.highlights
      .split("\n")
      .map((l) => l.replace(/^[-*]\s*/, "").trim())
      .filter(Boolean),
    blockers: draft.blockers,
    next_steps: draft.next_steps,
    additional_notes: draft.additional_notes,
    status: "pending",
    completion_date: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    financial_records:
      draft.records > 0
        ? [
            {
              category: "Team Compensation",
              records: Array.from({ length: draft.records }, (_, i) => ({
                category: "Team Compensation",
                description: `Disclosed record ${i + 1}`,
                token: funding.token,
                amount: perRecord,
                receiver: funding.address ?? "0x" + "0".repeat(40),
              })),
            },
          ]
        : [],
  };

  return (
    <div className="submit-project-update-wizard" data-step={step}>
      {(value === "general" || value === "financials") && (
        <>
          <GvSubmitProjectUpdate signedIn initialValues={undefined} funds={funds} />
          <div
            className="submit-project-update-wizard__controls"
            role="group"
            aria-label="Update steps"
          >
            <span className="submit-project-update-wizard__hint">
              {value === "general"
                ? "Step 1 of 4 \u{2014} General (project health + narrative)"
                : "Step 2 of 4 \u{2014} Financials (funds released / disclosed)"}
            </span>
            {value === "financials" && (
              <button
                type="button"
                className="submit-project-update-wizard__btn"
                onClick={() => send({ type: "BACK" })}
              >
                Back
              </button>
            )}
            <button
              type="button"
              className="submit-project-update-wizard__btn submit-project-update-wizard__btn--primary"
              onClick={() => {
                if (value === "general") {
                  send({ type: "SET_GENERAL", health: draft.health, fields: deepLinkDraft() });
                  send({ type: "NEXT" });
                } else {
                  send({
                    type: "SET_FINANCIALS",
                    csv: csvHeader,
                    disclosed: draft.disclosed,
                    records: draft.records,
                  });
                  send({ type: "NEXT" });
                }
              }}
            >
              {value === "general" ? "Continue to financials" : "Preview update"}
            </button>
          </div>
        </>
      )}

      {value === "preview" && (
        <>
          <GvProjectUpdateDetail
            {...({
              update: previewUpdate,
              project: { title: project.title, status: project.status },
              state: "ready",
            } as unknown as React.ComponentProps<typeof GvProjectUpdateDetail>)}
          />
          <div
            className="submit-project-update-wizard__controls"
            role="group"
            aria-label="Preview actions"
          >
            <span className="submit-project-update-wizard__hint">
              Step 3 of 4 &#x2014; Preview (simulated publish next)
            </span>
            <button
              type="button"
              className="submit-project-update-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back to edit
            </button>
            <button
              type="button"
              className="submit-project-update-wizard__btn submit-project-update-wizard__btn--primary"
              onClick={() => send({ type: "PUBLISH" })}
            >
              Publish update
            </button>
          </div>
        </>
      )}

      {value === "publishing" && (
        <div className="submit-project-update-wizard__progress" role="status" aria-live="polite">
          <span className="submit-project-update-wizard__spinner" aria-hidden="true" />
          <p>Publishing your update&#x2026; (simulated &#x2014; no governance write API)</p>
        </div>
      )}

      {value === "publishError" && (
        <div className="submit-project-update-wizard__error" role="alert">
          <p>Could not publish: {state.context.error ?? "unknown error"}</p>
          <div className="submit-project-update-wizard__controls">
            <button
              type="button"
              className="submit-project-update-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back to preview
            </button>
            <button
              type="button"
              className="submit-project-update-wizard__btn submit-project-update-wizard__btn--primary"
              onClick={() => send({ type: "RETRY" })}
            >
              Retry
            </button>
          </div>
        </div>
      )}

      {value === "success" && (
        <>
          <GvProposalDetailSuccessOutcomeScreens variant="update" />
          <p className="submit-project-update-wizard__stub" role="note">
            Simulated publish &#x2014; stub update id{" "}
            <code>{state.context.result?.updateId}</code> for project{" "}
            <code>{project.id}</code>.
          </p>
        </>
      )}
    </div>
  );
}
