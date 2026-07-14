import { useState } from "react";

import {
  DURATION_PRESETS,
  durationMsFor,
  shortAddress,
  validateReason,
  type UserAction,
} from "@data/lib/catalyst/admin/user-bans";

export type UserBanActionSubmit = {
  action: UserAction;
  address: string;
  reason: string;
  durationMs: number | null;
  customMessage: string | null;
};

export type UserBanActionFormProps = {
  address: string;
  isBanned: boolean;
  onSubmit: (s: UserBanActionSubmit) => void;
  onBack?: () => void;
  error?: string | null;
};

const ACTION_TABS: { id: UserAction; label: string }[] = [
  { id: "ban", label: "Ban" },
  { id: "warn", label: "Warn" },
  { id: "unban", label: "Lift ban" },
];

export default function UserBanActionForm({
  address,
  isBanned,
  onSubmit,
  onBack,
  error,
}: UserBanActionFormProps) {
  const [action, setAction] = useState<UserAction>(isBanned ? "unban" : "ban");
  const [reason, setReason] = useState("");
  const [durationId, setDurationId] = useState("permanent");
  const [customMessage, setCustomMessage] = useState("");
  const [touched, setTouched] = useState(false);

  const reasonRequired = action !== "unban";
  const reasonErrors = reasonRequired ? validateReason(reason) : {};
  const canSubmit = !reasonRequired || !reasonErrors.reason;

  function submit() {
    setTouched(true);
    if (!canSubmit) return;
    onSubmit({
      action,
      address,
      reason: reasonRequired ? reason.trim() : "Lift ban",
      durationMs: action === "ban" ? durationMsFor(durationId) : null,
      customMessage: action === "ban" && customMessage.trim() ? customMessage.trim() : null,
    });
  }

  return (
    <div className="au-field" aria-label="Moderator action">
      <div className="au-modal__header">
        <span
          className="au-modal__avatar u-avatar"
          style={{ "--sz": "48px" } as React.CSSProperties}
          aria-hidden="true"
        />
        <div className="au-modal__headertext">
          <span className="au-modal__name">{shortAddress(address)}</span>
          <span className="au-modal__address u-truncate">{address}</span>
        </div>
      </div>

      <div className="au-bar__tabs" role="tablist" aria-label="Action">
        {ACTION_TABS.map((t) => {
          if (t.id === "unban" && !isBanned) return null;
          return (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={action === t.id}
              className={"au-bar__tab" + (action === t.id ? " is-active" : "")}
              onClick={() => setAction(t.id)}
            >
              {t.label}
            </button>
          );
        })}
      </div>

      {action !== "unban" && (
        <>
          <label className="au-field__label" htmlFor="op-reason">
            Reason
          </label>
          <input
            id="op-reason"
            className={"au-field__input" + (touched && reasonErrors.reason ? " is-error" : "")}
            placeholder={action === "ban" ? "Why is this user being banned?" : "Why is this user being warned?"}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            aria-label="Reason"
          />
          <span className={"au-field__help" + (touched && reasonErrors.reason ? " is-error" : "")}>
            {touched && reasonErrors.reason ? reasonErrors.reason : " "}
          </span>
        </>
      )}

      {action === "ban" && (
        <>
          <label className="au-field__label" htmlFor="op-duration">
            Duration
          </label>
          <select
            id="op-duration"
            className="au-field__input"
            value={durationId}
            onChange={(e) => setDurationId(e.target.value)}
            aria-label="Ban duration"
          >
            {DURATION_PRESETS.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </select>

          <label className="au-field__label" htmlFor="op-message">
            Custom message (optional)
          </label>
          <textarea
            id="op-message"
            className="au-field__input"
            placeholder="Shown to the banned user (optional)"
            value={customMessage}
            onChange={(e) => setCustomMessage(e.target.value)}
            aria-label="Custom message"
            rows={2}
          />
        </>
      )}

      {error && (
        <div className="au-alert au-alert--error" role="alert">
          <span className="au-alert__msg">{error}</span>
        </div>
      )}

      <div className="au-modal__footer">
        {onBack && (
          <button type="button" className="au-btn au-btn--secondary" onClick={onBack}>
            Back
          </button>
        )}
        <button
          type="button"
          className="au-btn au-btn--primary"
          onClick={submit}
          disabled={!canSubmit}
        >
          Review {action === "ban" ? "ban" : action === "warn" ? "warning" : "lift"}
        </button>
      </div>
    </div>
  );
}
