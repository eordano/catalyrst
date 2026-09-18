import { useState } from "react";

import Button from "../../atoms/Button";
import type {
  ModerationDecision,
  Option,
  ReportCard,
} from "./AdReportTypes";
import {
  MAX_NOTE_LENGTH,
  MODERATION_DECISIONS,
  decisionLabel,
  decisionTone,
} from "./AdReportTypes";

type ModerationDecisionBarProps = {
  card: ReportCard;
  resolutions: Option[];
  decision: ModerationDecision;
  disablePlace: boolean;
  error?: string;
  onDecide: (decision: ModerationDecision, resolution?: string, notes?: string) => void;
  onToggleDisable: (disabled: boolean) => void;
  onConfirm: () => void;
  onCancel: () => void;
};

export default function AdModerationDecisionBar({
  card,
  resolutions,
  decision,
  disablePlace,
  error,
  onDecide,
  onToggleDisable,
  onConfirm,
  onCancel,
}: ModerationDecisionBarProps) {
  const [resolution, setResolution] = useState("");
  const [notes, setNotes] = useState("");

  const closing = decision !== "reopen";
  const canDisable = Boolean(card.entityId);

  function pick(next: ModerationDecision) {
    onDecide(next, resolution || undefined, notes.trim() || undefined);
  }

  return (
    <div className="adm-stack" role="region" aria-label="Moderation decision">
      <h2 className="adm-card__title">Decide on report #{card.id}</h2>

      <div className="adm-actions adm-actions--start" role="radiogroup" aria-label="Decision">
        {MODERATION_DECISIONS.map((d) => (
          <button
            key={d}
            type="button"
            role="radio"
            aria-checked={decision === d}
            className={"adm-choice" + (decision === d ? " is-active" : "")}
            data-tone={decisionTone(d)}
            onClick={() => pick(d)}
          >
            {decisionLabel(d)}
            {d === "action" && <span className="adm-choice__hint">+ disable place</span>}
          </button>
        ))}
      </div>

      {closing && (
        <label className="adm-field">
          <span className="adm-field__label">Resolution</span>
          <select
            className="adm-input"
            value={resolution}
            onChange={(e) => {
              setResolution(e.target.value);
              onDecide(decision, e.target.value || undefined, notes.trim() || undefined);
            }}
          >
            <option value="">Select a resolution&#x2026;</option>
            {resolutions.map((r) => (
              <option key={r.code} value={r.code}>
                {r.label}
              </option>
            ))}
          </select>
        </label>
      )}

      <label className="adm-field">
        <span className="adm-field__label">
          Resolution note {closing ? "" : "(reopen reason)"}
        </span>
        <textarea
          className="adm-input"
          rows={2}
          maxLength={MAX_NOTE_LENGTH}
          placeholder="Recorded in moderator_notes; the creator may be notified."
          value={notes}
          onChange={(e) => {
            setNotes(e.target.value);
            onDecide(decision, resolution || undefined, e.target.value.trim() || undefined);
          }}
        />
      </label>

      {canDisable && (
        <label className="adm-check">
          <input
            type="checkbox"
            checked={disablePlace}
            onChange={(e) => onToggleDisable(e.target.checked)}
          />
          <span>
            Also disable (soft-delete) <strong>{card.placeTitle}</strong> via{" "}
            <code>PATCH /api/places/{card.entityId}/disable</code>
          </span>
        </label>
      )}

      {error && (
        <p className="adm-notice" data-tone="bad" role="alert">
          Commit failed: {error}. Please try again.
        </p>
      )}

      <p className="adm-card__text adm-dim">
        Commits <code>PATCH /places/api/reports/{card.id}</code>
        {disablePlace && canDisable ? (
          <>
            {" + "}
            <code>PATCH /places/api/places/{card.entityId}/disable</code>
          </>
        ) : null}{" "}
        <em>(admin-bearer gated &#x2014; fails closed 403 without a bearer)</em>.
      </p>

      <div className="adm-actions">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          variant="primary"
          tone={decision === "action" ? "danger" : undefined}
          onClick={onConfirm}
        >
          Confirm {decisionLabel(decision).toLowerCase()}
        </Button>
      </div>
    </div>
  );
}
