import type { ComponentType, ReactNode } from "react";

import GvSubmitBid from "../pages/GvSubmitBid";
import GvProposalDetailSuccessOutcomeScreens from "../components/GvProposalDetailSuccessOutcomeScreens";
import GovernanceChrome from "../frames/GovernanceChrome";
import "./gvsubmitbidview.css";

const STEP_LABELS: { slug: string; label: string }[] = [
  { slug: "parents", label: "Parents" },
  { slug: "funding", label: "Funding" },
  { slug: "general", label: "General" },
  { slug: "review", label: "Review" },
  { slug: "submitting", label: "Submit" },
  { slug: "success", label: "Done" },
];

export type GvBidParent = {
  id: string;
  kind: string;
  title: string;
  url: string;
  finishedLabel: string;
  votingPowerLabel: string;
};

type GvBidTenderOption = { id: string; title: string; href: string };

type LinkComponentProps = {
  to: string;
  prefetch?: "intent";
  role?: string;
  className?: string;
  "data-active"?: boolean;
  "aria-current"?: "true";
  children?: ReactNode;
};

const FALLBACK_PARENT: GvBidParent = {
  id: "",
  kind: "",
  title: "",
  url: "",
  finishedLabel: "passed",
  votingPowerLabel: "",
};

type GvSubmitBidViewProps = {
  value?: string;
  step?: string;
  copy?: { title: string; description: string };
  account?: { label: string; votingPower: number };
  parents?: { pitch: GvBidParent; tender: GvBidParent };
  tenders?: GvBidTenderOption[];
  fallback?: boolean;
  submitError?: string;
  success?: { title: string; description: string; note: string };
  error?: string;
  resultProposalId?: string;
  LinkComponent?: ComponentType<LinkComponentProps>;
  onTab?: (id: string) => void;
  onContinue?: () => void;
  onSetFunding?: () => void;
  onNext?: () => void;
  onSubmit?: () => void;
  onBack?: () => void;
  onRetry?: () => void;
};

export default function GvSubmitBidView({
  value = "parents",
  step = "parents",
  copy = { title: "", description: "" },
  account = { label: "", votingPower: 0 },
  parents = { pitch: FALLBACK_PARENT, tender: FALLBACK_PARENT },
  tenders = [],
  fallback = false,
  submitError = "",
  success = { title: "", description: "", note: "" },
  error = undefined,
  resultProposalId = undefined,
  LinkComponent = undefined,
  onTab = undefined,
  onContinue = undefined,
  onSetFunding = undefined,
  onNext = undefined,
  onSubmit = undefined,
  onBack = undefined,
  onRetry = undefined,
}: GvSubmitBidViewProps) {
  const completed = STEP_LABELS.findIndex((s) => s.slug === step);

  const stepRail = (
    <div className="gv-bid-wizard__steps" role="list" aria-label="Bid proposal steps">
      {STEP_LABELS.map((s, i) => (
        <span
          key={s.slug}
          className="gv-bid-wizard__pip"
          role="listitem"
          data-active={s.slug === step}
          data-done={i < completed}
        >
          {s.label}
        </span>
      ))}
    </div>
  );

  if (value === "parents") {
    const { pitch, tender } = parents;
    const unavailable = fallback || tenders.length === 0;
    const ParentCard = (p: typeof pitch) => (
      <a className="gv-bid-wizard__parentcard" href={p.url} target="_blank" rel="noreferrer">
        <span className="gv-bid-wizard__parentkind">{p.kind}</span>
        <div className="gv-bid-wizard__parenttitle">{p.title}</div>
        <div className="gv-bid-wizard__parentsub">
          <span>
            {p.finishedLabel === "rejected" ? "Rejected" : "Passed"} with{" "}
            <strong>{p.votingPowerLabel} VP</strong>
          </span>
        </div>
      </a>
    );
    return (
      <div className="gv-bid-wizard" data-step={step}>
        <GovernanceChrome active="proposals" account={account.label} mana={account.votingPower.toLocaleString()} onTab={onTab}>
          {stepRail}
          <div className="gv-bid-wizard__panel" style={{ paddingBottom: 8 }}>
            <h1>{copy.title}</h1>
            <p>{copy.description}</p>
          </div>

          {unavailable ? (
            <div className="gv-bid-wizard__errorbox" role="alert">
              We couldn&rsquo;t load the parent Pitch/Tender proposals from
              governance right now. A Bid must descend from a passed parent
              Tender, so please try again later.
            </div>
          ) : (
            <>
              <div
                className="gv-bid-wizard__tenderselect"
                role="list"
                aria-label="Select the parent Tender"
              >
                {tenders.map((t) =>
                  LinkComponent ? (
                    <LinkComponent
                      key={t.id}
                      to={t.href}
                      prefetch="intent"
                      role="listitem"
                      className="gv-bid-wizard__tenderoption"
                      data-active={t.id === tender.id}
                      aria-current={t.id === tender.id ? "true" : undefined}
                    >
                      {t.title}
                    </LinkComponent>
                  ) : (
                    <a
                      key={t.id}
                      href={t.href}
                      role="listitem"
                      className="gv-bid-wizard__tenderoption"
                      data-active={t.id === tender.id}
                      aria-current={t.id === tender.id ? "true" : undefined}
                    >
                      {t.title}
                    </a>
                  ),
                )}
              </div>
              <div className="gv-bid-wizard__parents">
                {ParentCard(pitch)}
                {ParentCard(tender)}
              </div>
              <p className="gv-bid-wizard__hint">
                This Bid descends from the parent Pitch and Tender above. Pick a
                different Tender to retarget your bid, then continue to scope it.
              </p>
            </>
          )}

          <div className="gv-bid-wizard__controls" role="group" aria-label="Parents controls">
            <button
              type="button"
              className="gv-bid-wizard__btn gv-bid-wizard__btn--primary"
              onClick={() => onContinue?.()}
              disabled={unavailable}
            >
              Continue to funding
            </button>
          </div>
        </GovernanceChrome>
      </div>
    );
  }

  if (value === "funding" || value === "general" || value === "review" || value === "submitError") {
    return (
      <div className="gv-bid-wizard" data-step={step}>
        <GvSubmitBid />

        {stepRail}

        {value === "funding" && (
          <>
            <p className="gv-bid-wizard__hint">
              Step 1 &#x2014; Funding. Budget must be 100&#x2013;240,000 USD and the project
              duration 1&#x2013;12 months.
            </p>
            <div className="gv-bid-wizard__controls" role="group" aria-label="Funding controls">
              <button type="button" className="gv-bid-wizard__btn" onClick={() => onBack?.()}>
                Back
              </button>
              <button
                type="button"
                className="gv-bid-wizard__btn gv-bid-wizard__btn--primary"
                onClick={() => onSetFunding?.()}
              >
                Continue to general info
              </button>
            </div>
          </>
        )}

        {value === "general" && (
          <>
            <p className="gv-bid-wizard__hint">
              Step 2 &#x2014; General information, team, due-diligence budget breakdown and
              the final consent.
            </p>
            <div className="gv-bid-wizard__controls" role="group" aria-label="General controls">
              <button type="button" className="gv-bid-wizard__btn" onClick={() => onBack?.()}>
                Back
              </button>
              <button
                type="button"
                className="gv-bid-wizard__btn gv-bid-wizard__btn--primary"
                onClick={() => onNext?.()}
              >
                Continue to review
              </button>
            </div>
          </>
        )}

        {value === "review" && (
          <>
            <p className="gv-bid-wizard__hint">
              Step 3 &#x2014; review your bid, then submit. Submission is simulated (no
              on-chain transaction, no Snapshot proposal).
            </p>
            <div className="gv-bid-wizard__controls" role="group" aria-label="Review controls">
              <button type="button" className="gv-bid-wizard__btn" onClick={() => onBack?.()}>
                Back
              </button>
              <button
                type="button"
                className="gv-bid-wizard__btn gv-bid-wizard__btn--primary"
                onClick={() => onSubmit?.()}
              >
                Submit proposal (simulated)
              </button>
            </div>
          </>
        )}

        {value === "submitError" && (
          <>
            <div className="gv-bid-wizard__errorbox" role="alert">
              {submitError}
              {error ? ` (${error})` : null}
            </div>
            <div className="gv-bid-wizard__controls" role="group" aria-label="Error controls">
              <button type="button" className="gv-bid-wizard__btn" onClick={() => onBack?.()}>
                Back to review
              </button>
              <button
                type="button"
                className="gv-bid-wizard__btn gv-bid-wizard__btn--primary"
                onClick={() => onRetry?.()}
              >
                Retry
              </button>
            </div>
          </>
        )}
      </div>
    );
  }

  if (value === "submitting") {
    return (
      <div className="gv-bid-wizard" data-step={step}>
        <GovernanceChrome active="proposals" account={account.label} mana={account.votingPower.toLocaleString()} onTab={onTab}>
          {stepRail}
          <div className="gv-bid-wizard__panel" aria-live="polite">
            <div className="gv-bid-wizard__spinner" aria-hidden="true" />
            <h1>Submitting your bid&#x2026;</h1>
            <p>Creating the Bid proposal against the parent Tender (simulated).</p>
          </div>
        </GovernanceChrome>
      </div>
    );
  }

  return (
    <div className="gv-bid-wizard" data-step={step}>
      <GovernanceChrome active="proposals" account={account.label} mana={account.votingPower.toLocaleString()} onTab={onTab}>
        {stepRail}
        <div className="gv-bid-wizard__panel" aria-live="polite">
          <h1>{success.title}</h1>
          <p>{success.description}</p>
          {resultProposalId && (
            <p>
              Proposal id:{" "}
              <span style={{ fontFamily: "ui-monospace, monospace" }}>
                {resultProposalId}
              </span>
            </p>
          )}
          <span className="gv-bid-wizard__note">{success.note}</span>
        </div>
        <GvProposalDetailSuccessOutcomeScreens variant="bid" />
      </GovernanceChrome>
    </div>
  );
}
