import { useState } from "react";

import Button from "../../atoms/Button";
import type { CommunityDecision } from "./AdCommunityTypes";

type SuspendDecisionBarProps = {
  suspended: boolean | null;
  decision: CommunityDecision;
  onDecide: (decision: CommunityDecision, reason?: string) => void;
  onConfirm: () => void;
  onCancel: () => void;
  error?: string;
};

const MAX_REASON_LENGTH = 500;

export default function AdSuspendDecisionBar({
  decision,
  onDecide,
  onConfirm,
  onCancel,
  error,
}: SuspendDecisionBarProps) {
  const [reason, setReason] = useState("");
  const [reasonError, setReasonError] = useState(false);

  const isSuspend = decision === "suspend";

  function choose(next: CommunityDecision) {
    setReasonError(false);
    onDecide(next, next === "suspend" ? reason.trim() : undefined);
  }

  function confirm() {
    if (isSuspend && reason.trim().length === 0) {
      setReasonError(true);
      return;
    }
    onConfirm();
  }

  return (
    <div className="adm-card" role="region" aria-label="Moderation decision">
      <div className="adm-actions adm-actions--start" role="radiogroup" aria-label="Decision">
        <button
          type="button"
          role="radio"
          aria-checked={isSuspend}
          className={"adm-choice" + (isSuspend ? " is-active" : "")}
          data-tone="bad"
          onClick={() => choose("suspend")}
        >
          Suspend
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={!isSuspend}
          className={"adm-choice" + (!isSuspend ? " is-active" : "")}
          data-tone="ok"
          onClick={() => choose("unsuspend")}
        >
          Unsuspend
        </button>
      </div>

      {isSuspend ? (
        <label className={"adm-field" + (reasonError ? " is-error" : "")}>
          <span className="adm-field__label">Suspension reason*</span>
          <textarea
            className="adm-input"
            rows={2}
            maxLength={MAX_REASON_LENGTH}
            placeholder="Recorded on the community for audit. Be specific."
            value={reason}
            onChange={(e) => {
              setReason(e.target.value);
              setReasonError(false);
              onDecide("suspend", e.target.value.trim());
            }}
          />
          {reasonError && (
            <span className="adm-field__help is-error" role="alert">
              A reason is required to suspend.
            </span>
          )}
        </label>
      ) : (
        <p className="adm-card__text">
          Clears the suspension via{" "}
          <code>POST /v1/admin/communities/&#123;id&#125;/unsuspend</code>, a real
          write gated by this node&apos;s admin bearer token.
        </p>
      )}

      {error && (
        <p className="adm-bad" role="alert">
          Moderation failed: {error}. Please try again.
        </p>
      )}

      <div className="adm-actions">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="primary" tone={isSuspend ? "danger" : "success"} onClick={confirm}>
          {isSuspend ? "Confirm suspend" : "Confirm unsuspend"}
        </Button>
      </div>
    </div>
  );
}
