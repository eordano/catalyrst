import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import AdModerationDecisionBar from "./AdModerationDecisionBar";
import AdReportQueue from "./AdReportQueue";
import AdReportReviewPanel from "./AdReportReviewPanel";
import type {
  ModeratePlacesStateValue,
  ModerationDecision,
  Option,
  QueueBuckets,
  ReportCard,
} from "./AdReportTypes";
import "../admin.css";
import "./placesmoderation.css";

type AdModeratePlacesViewProps = {
  step: string;
  value: ModeratePlacesStateValue;
  buckets: QueueBuckets;
  reasons: Option[];
  resolutions: Option[];
  total: number;
  activeId?: string;
  activeCard?: ReportCard;
  decision: ModerationDecision;
  disablePlace: boolean;
  error?: string;
  resultStatus?: string;
  resultPlaceDisabled?: boolean;
  onOpen: (reportId: string) => void;
  onClose: () => void;
  onDecide: (decision: ModerationDecision, resolution?: string, notes?: string) => void;
  onToggleDisable: (disabled: boolean) => void;
  onConfirm: () => void;
  onCancel: () => void;
  onContinue: () => void;
};

export default function AdModeratePlacesView({
  step,
  value,
  buckets,
  reasons,
  resolutions,
  total,
  activeId = undefined,
  activeCard = undefined,
  decision,
  disablePlace,
  error = undefined,
  resultStatus = undefined,
  resultPlaceDisabled = undefined,
  onOpen,
  onClose,
  onDecide,
  onToggleDisable,
  onConfirm,
  onCancel,
  onContinue,
}: AdModeratePlacesViewProps) {
  return (
    <div className="adm adm-places" data-step={step} data-state={value}>
      <AdReportQueue
        buckets={buckets}
        reasons={reasons}
        total={total}
        activeId={activeId}
        onOpen={onOpen}
      />

      <div role="group" aria-label="Moderation wizard">
        {value === "reviewReport" && activeCard && (
          <div className="adm-card adm-card--solid adm-card--float">
            <AdReportReviewPanel
              card={activeCard}
              reasons={reasons}
              onClose={onClose}
            />
            <div className="adm-card__foot adm-stack">
              <p className="adm-card__text">Choose a decision to continue.</p>
              <div className="adm-actions adm-actions--start">
                <Button variant="secondary" tone="success" onClick={() => onDecide("resolve")}>
                  Resolve
                </Button>
                <Button variant="secondary" onClick={() => onDecide("dismiss")}>
                  Dismiss
                </Button>
                <Button variant="secondary" tone="danger" onClick={() => onDecide("action")}>
                  Action + disable
                </Button>
                {activeCard.status !== "open" && (
                  <Button variant="secondary" onClick={() => onDecide("reopen")}>
                    Reopen
                  </Button>
                )}
              </div>
            </div>
          </div>
        )}

        {value === "decision" && activeCard && (
          <div className="adm-card adm-card--solid adm-card--float">
            <AdModerationDecisionBar
              card={activeCard}
              resolutions={resolutions}
              decision={decision}
              disablePlace={disablePlace}
              error={error}
              onDecide={onDecide}
              onToggleDisable={onToggleDisable}
              onConfirm={onConfirm}
              onCancel={onCancel}
            />
          </div>
        )}

        {value === "submitting" && (
          <div className="adm-card adm-card--solid adm-card--float" role="status">
            <div className="adm-actions adm-actions--start">
              <Spinner size={22} aria-hidden="true" />
              <p className="adm-card__text">
                Applying decision&#x2026; <em>(PATCH /places/api/reports/{activeId})</em>
              </p>
            </div>
          </div>
        )}

        {value === "moderated" && activeCard && (
          <div className="adm-card adm-card--solid adm-card--float" role="status" aria-live="polite">
            <h2 className="adm-card__title">Decision recorded</h2>
            <p className="adm-card__text">
              <strong>{activeCard.placeTitle}</strong> &#x2014; report #{activeCard.id}{" "}
              {resultStatus ?? "updated"}
              {resultPlaceDisabled ? "; place disabled" : ""}.
            </p>
            <div className="adm-actions adm-actions--start">
              <Button variant="primary" onClick={onContinue}>
                Back to queue
              </Button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
