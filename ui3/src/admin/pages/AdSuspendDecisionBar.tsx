import { useState } from "react";

import Button from "../../atoms/Button";
import type { CommunityDecision } from "./AdCommunityTypes";

export type SuspendDecisionBarProps = {
  /** Null when the source listing reported no suspension state at all. */
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
    <div className="sdb" role="region" aria-label="Moderation decision">
      <div className="sdb__choices" role="radiogroup" aria-label="Decision">
        <button
          type="button"
          role="radio"
          aria-checked={isSuspend}
          className={"sdb__choice sdb__choice--suspend" + (isSuspend ? " is-active" : "")}
          onClick={() => choose("suspend")}
        >
          Suspend
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={!isSuspend}
          className={"sdb__choice sdb__choice--unsuspend" + (!isSuspend ? " is-active" : "")}
          onClick={() => choose("unsuspend")}
        >
          Unsuspend
        </button>
      </div>

      {isSuspend ? (
        <label className="sdb__reasonlabel">
          Suspension reason*
          <textarea
            className={"sdb__reason" + (reasonError ? " is-error" : "")}
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
            <span className="sdb__error" role="alert">
              A reason is required to suspend.
            </span>
          )}
        </label>
      ) : (
        <p className="sdb__text">
          Clears the suspension via{" "}
          <code>POST /v1/admin/communities/&#123;id&#125;/unsuspend</code>, a real
          write gated by this node&apos;s admin bearer token.
        </p>
      )}

      {error && (
        <p className="sdb__error" role="alert">
          Moderation failed: {error}. Please try again.
        </p>
      )}

      <div className="sdb__actions">
        <Button variant="secondary" className="sdb__btn" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          variant="primary"
          className={"sdb__btn--primary" + (isSuspend ? " sdb__btn--danger" : "")}
          onClick={confirm}
        >
          {isSuspend ? "Confirm suspend" : "Confirm unsuspend"}
        </Button>
      </div>
    </div>
  );
}
