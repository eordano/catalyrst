import { useState } from "react";

import Button from "@ui/atoms/Button";
import { Avatar } from "@ui/atoms/primitives";
import "@ui/admin/admin.css";

import {
  DURATION_PRESETS,
  durationMsFor,
  shortAddress,
  validateReason,
  type UserAction,
} from "@data/lib/catalyst/admin/user-bans";

type UserBanActionSubmit = {
  action: UserAction;
  address: string;
  reason: string;
  durationMs: number | null;
  customMessage: string | null;
};

type UserBanActionFormProps = {
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

const TONES: Partial<Record<UserAction, "danger" | "success">> = {
  ban: "danger",
  unban: "success",
};

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
  const reasonError = touched && reasonErrors.reason ? " is-error" : "";

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
    <div className="adm-card" aria-label="Moderator action">
      <div className="adm-card__head">
        <Avatar seed={address} size={48} />
        <div>
          <div className="adm-card__title">{shortAddress(address)}</div>
          <div className="adm-mono adm-dim u-truncate">{address}</div>
        </div>
      </div>

      <div className="adm-pills" role="tablist" aria-label="Action">
        {ACTION_TABS.map((t) => {
          if (t.id === "unban" && !isBanned) return null;
          return (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={action === t.id}
              className={"adm-pill" + (action === t.id ? " is-active" : "")}
              onClick={() => setAction(t.id)}
            >
              {t.label}
            </button>
          );
        })}
      </div>

      {action !== "unban" && (
        <div className={"adm-field" + reasonError}>
          <label className="adm-field__label" htmlFor="op-reason">
            Reason
          </label>
          <input
            id="op-reason"
            className="adm-input"
            placeholder={action === "ban" ? "Why is this user being banned?" : "Why is this user being warned?"}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            aria-label="Reason"
          />
          <span className={"adm-field__help" + reasonError}>
            {touched && reasonErrors.reason ? reasonErrors.reason : " "}
          </span>
        </div>
      )}

      {action === "ban" && (
        <>
          <div className="adm-field">
            <label className="adm-field__label" htmlFor="op-duration">
              Duration
            </label>
            <select
              id="op-duration"
              className="adm-input"
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
          </div>

          <div className="adm-field">
            <label className="adm-field__label" htmlFor="op-message">
              Custom message (optional)
            </label>
            <textarea
              id="op-message"
              className="adm-input"
              placeholder="Shown to the banned user (optional)"
              value={customMessage}
              onChange={(e) => setCustomMessage(e.target.value)}
              aria-label="Custom message"
              rows={2}
            />
          </div>
        </>
      )}

      {error && (
        <div className="adm-notice" data-tone="bad" role="alert">
          <p>{error}</p>
        </div>
      )}

      <div className="adm-actions">
        {onBack && (
          <Button variant="secondary" onClick={onBack}>
            Back
          </Button>
        )}
        <Button tone={TONES[action]} onClick={submit} disabled={!canSubmit}>
          Review {action === "ban" ? "ban" : action === "warn" ? "warning" : "lift"}
        </Button>
      </div>
    </div>
  );
}
