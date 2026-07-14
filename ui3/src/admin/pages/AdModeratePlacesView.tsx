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
import "../../web/pages/stwhatsonadminpendingevents.css";
import "./placesmoderation.css";

export type AdModeratePlacesViewProps = {
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
    <div className="mpw" data-step={step} data-state={value}>
      <AdReportQueue
        buckets={buckets}
        reasons={reasons}
        total={total}
        activeId={activeId}
        onOpen={onOpen}
      />

      <div className="mpw-layer" role="group" aria-label="Moderation wizard">
        {value === "reviewReport" && activeCard && (
          <div className="mpw-panel">
            <AdReportReviewPanel
              card={activeCard}
              reasons={reasons}
              onClose={onClose}
            />
            <div className="mpw-panel__decide">
              <p className="mpw-panel__text">Choose a decision to continue.</p>
              <div className="mpw-panel__decidebtns">
                <button
                  type="button"
                  className="mdb-btn mdb-btn--resolve"
                  onClick={() => onDecide("resolve")}
                >
                  Resolve
                </button>
                <button
                  type="button"
                  className="mdb-btn mdb-btn--dismiss"
                  onClick={() => onDecide("dismiss")}
                >
                  Dismiss
                </button>
                <button
                  type="button"
                  className="mdb-btn mdb-btn--action"
                  onClick={() => onDecide("action")}
                >
                  Action + disable
                </button>
                {activeCard.status !== "open" && (
                  <button
                    type="button"
                    className="mdb-btn mdb-btn--reopen"
                    onClick={() => onDecide("reopen")}
                  >
                    Reopen
                  </button>
                )}
              </div>
            </div>
          </div>
        )}

        {value === "decision" && activeCard && (
          <div className="mpw-panel">
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
          <div className="mpw-panel mpw-panel--submitting" role="status">
            <Spinner size={22} aria-hidden="true" />
            <p className="mpw-panel__text">
              Applying decision&#x2026; <em>(PATCH /places/api/reports/{activeId})</em>
            </p>
          </div>
        )}

        {value === "moderated" && activeCard && (
          <div className="mpw-panel mpw-panel--done" role="status" aria-live="polite">
            <h2 className="mpw-panel__title">Decision recorded</h2>
            <p className="mpw-panel__text">
              <strong>{activeCard.placeTitle}</strong> &#x2014; report #{activeCard.id}{" "}
              {resultStatus ?? "updated"}
              {resultPlaceDisabled ? "; place disabled" : ""}.
            </p>
            <button
              type="button"
              className="mdb-btn mdb-btn--confirm"
              onClick={onContinue}
            >
              Back to queue
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
