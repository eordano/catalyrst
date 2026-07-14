import type React from "react";

import GovernanceChrome from "../frames/GovernanceChrome";
import SubmitProposalForm from "../components/SubmitProposalForm";
import "./gvsubmitbannameview.css";

const STEP_LABELS: { slug: string; label: string }[] = [
  { slug: "details", label: "Name" },
  { slug: "description", label: "Rationale" },
  { slug: "review", label: "Review" },
  { slug: "submitting", label: "Submit" },
  { slug: "success", label: "Done" },
];

export type GvBanNameDraft = {
  name: string;
  description: string;
  coAuthors: string[];
};

type GvBanNameCopy = {
  title: string;
  intro: string;
  nameLabel: string;
  namePlaceholder: string;
  descriptionLabel: string;
  descriptionDetail: string;
  descriptionPlaceholder: string;
  submitLabel: string;
  account: { label: string };
  schema: {
    name: { max: number };
    description: { min: number; max: number };
    coAuthors: { max: number };
  };
};

type GvSubmitBanNameViewProps = {
  value?: string;
  step?: string;
  ctx?: GvBanNameCopy;
  draft?: GvBanNameDraft;
  errors?: Record<string, string>;
  error?: string;
  resultProposalId?: string;
  onTab?: (id: string) => void;
  onSubmitName?: (name: string) => void;
  onSubmitDescription?: (details: { description: string; coAuthors: string[] }) => void;
  onConfirm?: () => void;
  onBack?: () => void;
  onRetry?: () => void;
};

const FALLBACK_CTX: GvBanNameCopy = {
  title: "",
  intro: "",
  nameLabel: "",
  namePlaceholder: "",
  descriptionLabel: "",
  descriptionDetail: "",
  descriptionPlaceholder: "",
  submitLabel: "",
  account: { label: "" },
  schema: {
    name: { max: 15 },
    description: { min: 20, max: 250 },
    coAuthors: { max: 5 },
  },
};

export default function GvSubmitBanNameView({
  value = "details",
  step = "details",
  ctx = FALLBACK_CTX,
  draft = { name: "", description: "", coAuthors: [] },
  errors = {},
  error = undefined,
  resultProposalId = undefined,
  onTab = undefined,
  onSubmitName = undefined,
  onSubmitDescription = undefined,
  onConfirm = undefined,
  onBack = undefined,
  onRetry = undefined,
}: GvSubmitBanNameViewProps) {
  return (
    <GovernanceChrome active="proposals" account={ctx.account.label} signedIn onTab={onTab}>
      <div className="submit-ban-name-wizard" data-step={step}>
        <ol className="submit-ban-name-wizard__steps" aria-label="Ban Name submission steps">
          {STEP_LABELS.map(({ slug, label }) => (
            <li
              key={slug}
              className="submit-ban-name-wizard__step"
              aria-current={slug === step ? "step" : undefined}
              data-active={slug === step ? "true" : "false"}
            >
              {label}
            </li>
          ))}
        </ol>

        {(value === "details" || value === "description") && (
          <SubmitProposalForm
            {...({
              key: value,
              title: ctx.title,
              subtitle: value === "details" ? "Step 1 — Name to ban" : "Step 2 — Rationale",
              description: ctx.intro,
              account: ctx.account.label,
              submitLabel: value === "details" ? "Continue" : "Continue to review",
              secondaryLabel: value === "description" ? "Back" : undefined,
              onSecondary:
                value === "description" ? () => onBack?.() : undefined,
              sections:
                value === "details"
                  ? [
                      {
                        type: "text",
                        name: "name",
                        label: ctx.nameLabel,
                        placeholder: ctx.namePlaceholder,
                        maxLength: ctx.schema.name.max,
                        value: draft.name,
                        error: errors.name,
                      },
                    ]
                  : [
                      {
                        type: "markdown",
                        name: "description",
                        label: ctx.descriptionLabel,
                        markdown: true,
                        sublabel: ctx.descriptionDetail,
                        placeholder: ctx.descriptionPlaceholder,
                        maxLength: ctx.schema.description.max,
                        minLength: ctx.schema.description.min,
                        value: draft.description,
                        error: errors.description,
                      },
                      {
                        type: "coauthors",
                        name: "coAuthors",
                        label: "Co-authors",
                        optional: true,
                        max: ctx.schema.coAuthors.max,
                        value: draft.coAuthors,
                      },
                    ],
              onSubmit: (vals: Record<string, unknown>) => {
                if (value === "details") {
                  onSubmitName?.(String(vals.name ?? draft.name ?? ""));
                } else {
                  const co = Array.isArray(vals.coAuthors)
                    ? (vals.coAuthors as string[])
                    : draft.coAuthors;
                  onSubmitDescription?.({
                    description: String(vals.description ?? draft.description ?? ""),
                    coAuthors: co,
                  });
                }
              },
            } as unknown as React.ComponentProps<typeof SubmitProposalForm>)}
          />
        )}

        {value === "review" && (
          <section className="submit-ban-name-wizard__review" aria-label="Review proposal">
            <h2 className="submit-ban-name-wizard__h2">Review your Ban Name proposal</h2>
            <dl className="submit-ban-name-wizard__summary">
              <dt>Name to ban</dt>
              <dd>{draft.name}</dd>
              <dt>Description</dt>
              <dd>{draft.description}</dd>
              <dt>Co-authors</dt>
              <dd>{draft.coAuthors.length ? draft.coAuthors.join(", ") : "None"}</dd>
            </dl>
            <p className="submit-ban-name-wizard__note">
              Submission is simulated — no proposal is created on Snapshot/Aragon.
            </p>
            <div className="submit-ban-name-wizard__controls" role="group">
              <button
                type="button"
                className="submit-ban-name-wizard__btn"
                onClick={() => onBack?.()}
              >
                Back
              </button>
              <button
                type="button"
                className="submit-ban-name-wizard__btn submit-ban-name-wizard__btn--primary"
                onClick={() => onConfirm?.()}
              >
                {ctx.submitLabel}
              </button>
            </div>
          </section>
        )}

        {value === "submitting" && (
          <section className="submit-ban-name-wizard__status" aria-live="polite">
            <h2 className="submit-ban-name-wizard__h2">Submitting your proposal…</h2>
            <p className="submit-ban-name-wizard__note">
              Simulated createProposal (stub) — this never hits the network.
            </p>
          </section>
        )}

        {value === "success" && (
          <section className="submit-ban-name-wizard__status submit-ban-name-wizard__status--ok">
            <h2 className="submit-ban-name-wizard__h2">Proposal submitted</h2>
            <p className="submit-ban-name-wizard__note">
              Your Ban Name proposal for <strong>{draft.name}</strong> was created
              (simulated). Proposal id:{" "}
              <code>{resultProposalId ?? "—"}</code>.
            </p>
            <p className="submit-ban-name-wizard__note submit-ban-name-wizard__note--stub">
              This is a simulated outcome — governance has no createProposal
              endpoint reachable from this app (Snapshot + Aragon externally).
            </p>
          </section>
        )}

        {value === "error" && (
          <section className="submit-ban-name-wizard__status submit-ban-name-wizard__status--error">
            <h2 className="submit-ban-name-wizard__h2">Submission failed</h2>
            <p className="submit-ban-name-wizard__note">
              {error ?? "The simulated submission failed."}
            </p>
            <div className="submit-ban-name-wizard__controls" role="group">
              <button
                type="button"
                className="submit-ban-name-wizard__btn"
                onClick={() => onBack?.()}
              >
                Back to review
              </button>
              <button
                type="button"
                className="submit-ban-name-wizard__btn submit-ban-name-wizard__btn--primary"
                onClick={() => onRetry?.()}
              >
                Retry
              </button>
            </div>
          </section>
        )}
      </div>
    </GovernanceChrome>
  );
}
