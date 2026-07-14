import { useState } from "react";
import type React from "react";

import GovernanceChrome from "../frames/GovernanceChrome";
import SubmitProposalForm from "../components/SubmitProposalForm";
import GvProposalDetailSuccessOutcomeScreens from "../components/GvProposalDetailSuccessOutcomeScreens";
import { GOVERNANCE_FORMS } from "../../data/governanceForms";
import "./gvsubmitdraftview.css";

const BODY_NAMES = ["summary", "abstract", "motivation", "specification", "conclusion"] as const;

const STEP_LABELS: { slug: string; label: string }[] = [
  { slug: "intro", label: "Start" },
  { slug: "details", label: "Details" },
  { slug: "coauthors", label: "Co-authors" },
  { slug: "review", label: "Review" },
  { slug: "success", label: "Done" },
];

type DraftFieldConfig = {
  type: string;
  name: string;
  label?: string;
  sublabel?: string;
  placeholder?: string;
  maxLength?: number;
  minLength?: number;
  markdown?: boolean;
  [k: string]: unknown;
};
const DRAFT_SECTIONS = (GOVERNANCE_FORMS.draft as { sections: DraftFieldConfig[] }).sections;
const DRAFT_TITLE = (GOVERNANCE_FORMS.draft as { title: string }).title;
const DRAFT_DESCRIPTION = (GOVERNANCE_FORMS.draft as { description: unknown[] }).description;

const DETAILS_SECTIONS = DRAFT_SECTIONS.filter(
  (f) => f.name === "title" || (BODY_NAMES as readonly string[]).includes(f.name),
);

export type GvDraftCoAuthor = { addr: string };

export type GvDraftForm = {
  pollId?: string;
  title: string;
  bodies: Record<string, string>;
  coauthors: GvDraftCoAuthor[];
};

type GvLinkedPoll = { id: string; label: string };

type GvSubmitDraftViewProps = {
  value?: string;
  step?: string;
  source?: string;
  linkedPolls?: GvLinkedPoll[];
  vpThreshold?: number;
  vpUnit?: string;
  account?: { short: string; vp: number };
  limits?: { coauthorsMax: number };
  draft?: GvDraftForm;
  error?: string;
  resultProposalId?: string;
  onTab?: (id: string) => void;
  onStartDraft?: (pollId: string) => void;
  onSubmitDetails?: (details: { title: string; bodies: Record<string, string> }) => void;
  onContinueCoauthors?: (coauthors: GvDraftCoAuthor[]) => void;
  onSubmit?: () => void;
  onBack?: () => void;
  onRetry?: () => void;
};

export default function GvSubmitDraftView({
  value = "intro",
  step = "intro",
  source = "live",
  linkedPolls = [],
  vpThreshold = 0,
  vpUnit = "VP",
  account = { short: "Guest", vp: 0 },
  limits = { coauthorsMax: 5 },
  draft = { title: "", bodies: {}, coauthors: [] },
  error = undefined,
  resultProposalId = undefined,
  onTab = undefined,
  onStartDraft = undefined,
  onSubmitDetails = undefined,
  onContinueCoauthors = undefined,
  onSubmit = undefined,
  onBack = undefined,
  onRetry = undefined,
}: GvSubmitDraftViewProps) {
  const [loggedIn, setLoggedIn] = useState(true);
  const [selectedPoll, setSelectedPoll] = useState<string>(linkedPolls[0]?.id ?? "");
  const vpMet = account.vp >= vpThreshold;
  const gateOk = loggedIn && vpMet && !!selectedPoll;

  const [coauthors, setCoauthors] = useState<GvDraftCoAuthor[]>(draft.coauthors ?? []);

  const selectedPollLabel =
    linkedPolls.find((p) => p.id === draft.pollId)?.label ??
    draft.pollId ??
    "\u{2014}";

  return (
    <GovernanceChrome active="proposals" account={account.short} signedIn={loggedIn} onTab={onTab}>
      <div className="submit-draft-wizard" data-step={step} data-source={source}>
        <ol className="submit-draft-wizard__steps" aria-label="Draft proposal steps">
          {STEP_LABELS.map(({ slug, label }) => (
            <li
              key={slug}
              className="submit-draft-wizard__step"
              aria-current={slug === step ? "step" : undefined}
              data-active={slug === step ? "true" : "false"}
            >
              {label}
            </li>
          ))}
        </ol>

        {value === "intro" && (
          <section className="submit-draft-wizard__intro" aria-label={"Draft proposal \u{2014} start"}>
            <h1 className="submit-draft-wizard__h1">{DRAFT_TITLE}</h1>
            <p className="submit-draft-wizard__lead">
              The Draft Proposal presents a potential policy to the community in a
              structured format. It must link a passed Poll and requires at least{" "}
              <strong>
                {vpThreshold.toLocaleString("en-US")} {vpUnit}
              </strong>{" "}
              to submit.
            </p>

            {!loggedIn ? (
              <div className="submit-draft-wizard__gate" aria-label="Sign in required">
                <p className="submit-draft-wizard__note">
                  You need to sign in to submit a proposal.
                </p>
                <button
                  type="button"
                  className="submit-draft-wizard__btn submit-draft-wizard__btn--primary"
                  onClick={() => setLoggedIn(true)}
                >
                  Sign in
                </button>
              </div>
            ) : (
              <>
                {!vpMet && (
                  <p className="submit-draft-wizard__vpnotice" role="alert">
                    This action requires at least {vpThreshold.toLocaleString("en-US")}{" "}
                    {vpUnit}. You currently have {account.vp.toLocaleString("en-US")}{" "}
                    {vpUnit}.
                  </p>
                )}

                <label className="submit-draft-wizard__field">
                  <span className="submit-draft-wizard__label">Linked Poll</span>
                  <select
                    className="submit-draft-wizard__select"
                    value={selectedPoll}
                    onChange={(e) => setSelectedPoll(e.target.value)}
                    aria-label="Linked Poll"
                  >
                    <option value="" disabled>
                      Select a passed poll to link
                    </option>
                    {linkedPolls.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.label}
                      </option>
                    ))}
                  </select>
                </label>

                <div className="submit-draft-wizard__controls" role="group">
                  <button
                    type="button"
                    className="submit-draft-wizard__btn"
                    onClick={() => setLoggedIn(false)}
                  >
                    Sign out
                  </button>
                  <button
                    type="button"
                    className="submit-draft-wizard__btn submit-draft-wizard__btn--primary"
                    disabled={!gateOk}
                    onClick={() => onStartDraft?.(selectedPoll)}
                  >
                    Start draft
                  </button>
                </div>
              </>
            )}
          </section>
        )}

        {value === "details" && (
          <SubmitProposalForm
            {...({
              key: "details",
              title: DRAFT_TITLE,
              description: DRAFT_DESCRIPTION,
              account: account.short,
              submitLabel: "Continue",
              secondaryLabel: "Back",
              onSecondary: () => onBack?.(),
              sections: DETAILS_SECTIONS.map((f) => ({
                ...f,
                value:
                  f.name === "title"
                    ? draft.title
                    : draft.bodies[f.name] ?? "",
              })),
              onSubmit: (vals: Record<string, unknown>) => {
                const bodies: Record<string, string> = {};
                for (const name of BODY_NAMES) {
                  bodies[name] = String(vals[name] ?? draft.bodies[name] ?? "");
                }
                onSubmitDetails?.({
                  title: String(vals.title ?? draft.title ?? ""),
                  bodies,
                });
              },
            } as unknown as React.ComponentProps<typeof SubmitProposalForm>)}
          />
        )}

        {value === "coauthors" && (
          <section className="submit-draft-wizard__coauthors" aria-label={"Draft proposal \u{2014} co-authors"}>
            <h1 className="submit-draft-wizard__h1">Co-authors (optional)</h1>
            <p className="submit-draft-wizard__lead">
              Add the wallet addresses of anyone who co-authored this proposal.
              After you publish, co-authors are asked to confirm before being
              listed publicly. Up to {limits.coauthorsMax} co-authors.
            </p>

            <ul className="submit-draft-wizard__chips">
              {coauthors.map((c, i) => (
                <li key={i} className="submit-draft-wizard__chip">
                  {c.addr}
                  <button
                    type="button"
                    aria-label="Remove co-author"
                    onClick={() => setCoauthors(coauthors.filter((_, idx) => idx !== i))}
                  >
                    &#xD7;
                  </button>
                </li>
              ))}
            </ul>

            <div className="submit-draft-wizard__controls">
              <button
                type="button"
                className="submit-draft-wizard__btn"
                disabled={coauthors.length >= limits.coauthorsMax}
                onClick={() =>
                  setCoauthors([
                    ...coauthors,
                    {
                      addr:
                        "0x" +
                        Math.random().toString(16).slice(2, 6) +
                        "\u{2026}" +
                        Math.random().toString(16).slice(2, 6),
                    },
                  ])
                }
              >
                + Add co-author
              </button>
            </div>

            <div className="submit-draft-wizard__controls" role="group">
              <button
                type="button"
                className="submit-draft-wizard__btn"
                onClick={() => onBack?.()}
              >
                Back
              </button>
              <button
                type="button"
                className="submit-draft-wizard__btn submit-draft-wizard__btn--primary"
                onClick={() => onContinueCoauthors?.(coauthors)}
              >
                Continue
              </button>
            </div>
          </section>
        )}

        {value === "review" && (
          <section className="submit-draft-wizard__review" aria-label={"Draft proposal \u{2014} review"}>
            <h1 className="submit-draft-wizard__h1">Review your Draft proposal</h1>
            <dl className="submit-draft-wizard__summary">
              <dt>Linked Poll</dt>
              <dd>{selectedPollLabel}</dd>
              <dt>Title</dt>
              <dd>{draft.title || "\u{2014}"}</dd>
              <dt>Summary</dt>
              <dd>{draft.bodies.summary || "\u{2014}"}</dd>
              <dt>Co-authors</dt>
              <dd>
                {draft.coauthors.length
                  ? draft.coauthors.map((c) => c.addr).join(", ")
                  : "None"}
              </dd>
            </dl>
            <p className="submit-draft-wizard__note submit-draft-wizard__note--stub">
              Submission is simulated &#x2014; governance has no createProposal endpoint
              reachable from this app (Snapshot + Aragon run the DAO externally).
            </p>
            <div className="submit-draft-wizard__controls" role="group">
              <button
                type="button"
                className="submit-draft-wizard__btn"
                onClick={() => onBack?.()}
              >
                Back
              </button>
              <button
                type="button"
                className="submit-draft-wizard__btn submit-draft-wizard__btn--primary"
                onClick={() => onSubmit?.()}
              >
                Submit Draft proposal
              </button>
            </div>
          </section>
        )}

        {value === "submitting" && (
          <section className="submit-draft-wizard__status" aria-live="polite" aria-busy="true">
            <h1 className="submit-draft-wizard__h1">Submitting your Draft proposal&#x2026;</h1>
            <p className="submit-draft-wizard__note">
              Simulated createProposal (stub) &#x2014; this never hits the network.
            </p>
          </section>
        )}

        {value === "submitError" && (
          <section className="submit-draft-wizard__status submit-draft-wizard__status--error" role="alert">
            <h1 className="submit-draft-wizard__h1">
              There was an error submitting your proposal
            </h1>
            <p className="submit-draft-wizard__note">
              {error ?? "Please try again later."}
            </p>
            <div className="submit-draft-wizard__controls" role="group">
              <button
                type="button"
                className="submit-draft-wizard__btn"
                onClick={() => onBack?.()}
              >
                Back to review
              </button>
              <button
                type="button"
                className="submit-draft-wizard__btn submit-draft-wizard__btn--primary"
                onClick={() => onRetry?.()}
              >
                Retry
              </button>
            </div>
          </section>
        )}

        {value === "success" && (
          <section className="submit-draft-wizard__success" aria-label={"Draft proposal \u{2014} success"}>
            <GvProposalDetailSuccessOutcomeScreens variant="new" />
            <p className="submit-draft-wizard__note submit-draft-wizard__note--stub">
              Simulated proposal id:{" "}
              <code>{resultProposalId ?? "stub"}</code>
            </p>
          </section>
        )}
      </div>
    </GovernanceChrome>
  );
}
