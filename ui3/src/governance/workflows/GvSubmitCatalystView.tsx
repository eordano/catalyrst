import GvSubmitCatalyst from "../pages/GvSubmitCatalyst";
import GovernanceChrome from "../frames/GovernanceChrome";
import "./gvsubmitcatalystview.css";

const STEP_LABELS: { slug: string; label: string }[] = [
  { slug: "details", label: "Details" },
  { slug: "description", label: "Description" },
  { slug: "review", label: "Review" },
  { slug: "submitting", label: "Submit" },
  { slug: "success", label: "Done" },
];

export type GvCatalystRequest = "add" | "remove";

type GvSubmitCatalystViewProps = {
  value?: string;
  step?: string;
  request?: GvCatalystRequest;
  copy?: { title: string };
  descriptionField?: { min_length?: number | null; max_length?: number | null };
  success?: { title: string; lead: string; note: string };
  resultId?: string;
  onTab?: (id: string) => void;
  onSimulateInvalidDomain?: () => void;
  onContinueDetails?: () => void;
  onContinueDescription?: () => void;
  onSubmit?: () => void;
  onBack?: () => void;
  onRetry?: () => void;
};

export default function GvSubmitCatalystView({
  value = "details",
  step = "details",
  request = "add",
  copy = { title: "" },
  descriptionField = {},
  success = { title: "", lead: "", note: "" },
  resultId = undefined,
  onTab = undefined,
  onSimulateInvalidDomain = undefined,
  onContinueDetails = undefined,
  onContinueDescription = undefined,
  onSubmit = undefined,
  onBack = undefined,
  onRetry = undefined,
}: GvSubmitCatalystViewProps) {
  const completed = STEP_LABELS.findIndex((s) => s.slug === step);

  const stepRail = (
    <div className="gv-catalyst-wizard__steps" role="list" aria-label="Proposal steps">
      {STEP_LABELS.map((s, i) => (
        <span
          key={s.slug}
          className="gv-catalyst-wizard__pip"
          role="listitem"
          data-active={s.slug === step}
          data-done={i < completed}
        >
          {s.label}
        </span>
      ))}
    </div>
  );

  if (value === "details" || value === "description" || value === "review" || value === "error") {
    return (
      <div className="gv-catalyst-wizard" data-step={step} data-request={request}>
        <GvSubmitCatalyst
          catalystType={request}
          state="form"
          showError={value === "error"}
        />

        {stepRail}

        {value === "details" && (
          <>
            <p className="gv-catalyst-wizard__hint">
              Step 1 &#x2014; {copy.title}. Enter the owner address and node domain. The
              server status check is simulated.
            </p>
            <div className="gv-catalyst-wizard__controls" role="group" aria-label="Details controls">
              <button
                type="button"
                className="gv-catalyst-wizard__btn"
                onClick={() => onSimulateInvalidDomain?.()}
              >
                Simulate invalid domain
              </button>
              <button
                type="button"
                className="gv-catalyst-wizard__btn gv-catalyst-wizard__btn--primary"
                onClick={() => onContinueDetails?.()}
              >
                Continue to description
              </button>
            </div>
          </>
        )}

        {value === "description" && (
          <>
            <p className="gv-catalyst-wizard__hint">
              Step 2 &#x2014; rationale (markdown, {descriptionField.min_length}&#x2013;
              {descriptionField.max_length} chars) and optional co-authors.
            </p>
            <div className="gv-catalyst-wizard__controls" role="group" aria-label="Description controls">
              <button
                type="button"
                className="gv-catalyst-wizard__btn"
                onClick={() => onBack?.()}
              >
                Back
              </button>
              <button
                type="button"
                className="gv-catalyst-wizard__btn gv-catalyst-wizard__btn--primary"
                onClick={() => onContinueDescription?.()}
              >
                Continue to review
              </button>
            </div>
          </>
        )}

        {value === "review" && (
          <>
            <p className="gv-catalyst-wizard__hint">
              Step 3 &#x2014; review your {copy.title.toLowerCase()} proposal, then submit.
              Submission is simulated (no on-chain transaction).
            </p>
            <div className="gv-catalyst-wizard__controls" role="group" aria-label="Review controls">
              <button
                type="button"
                className="gv-catalyst-wizard__btn"
                onClick={() => onBack?.()}
              >
                Back
              </button>
              <button
                type="button"
                className="gv-catalyst-wizard__btn gv-catalyst-wizard__btn--primary"
                onClick={() => onSubmit?.()}
              >
                Submit proposal (simulated)
              </button>
            </div>
          </>
        )}

        {value === "error" && (
          <div className="gv-catalyst-wizard__controls" role="group" aria-label="Error controls">
            <button
              type="button"
              className="gv-catalyst-wizard__btn"
              onClick={() => onBack?.()}
            >
              Back to review
            </button>
            <button
              type="button"
              className="gv-catalyst-wizard__btn gv-catalyst-wizard__btn--primary"
              onClick={() => onRetry?.()}
            >
              Retry
            </button>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="gv-catalyst-wizard" data-step={step} data-request={request}>
      <GovernanceChrome active="proposals" onTab={onTab}>
        {stepRail}
        {value === "submitting" && (
          <div className="gv-catalyst-wizard__panel" aria-live="polite">
            <div className="gv-catalyst-wizard__spinner" aria-hidden="true" />
            <h1>Creating your proposal&#x2026;</h1>
            <p>Submitting the {copy.title.toLowerCase()} proposal (simulated).</p>
          </div>
        )}
        {value === "success" && (
          <div className="gv-catalyst-wizard__panel" aria-live="polite">
            <h1>{success.title}</h1>
            <p>{success.lead}</p>
            {resultId && (
              <p>
                Proposal id:{" "}
                <span className="gv-catalyst-wizard__id">{resultId}</span>
              </p>
            )}
            <span className="gv-catalyst-wizard__note">{success.note}</span>
          </div>
        )}
      </GovernanceChrome>
    </div>
  );
}
