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
} from "./AdReportTypes";

export type ModerationDecisionBarProps = {
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
    <div className="mdb" role="region" aria-label="Moderation decision">
      <h2 className="mdb__title">Decide on report #{card.id}</h2>

      <div className="mdb__decisions" role="radiogroup" aria-label="Decision">
        {MODERATION_DECISIONS.map((d) => (
          <button
            key={d}
            type="button"
            role="radio"
            aria-checked={decision === d}
            className={
              "mdb-btn mdb-btn--" + d + (decision === d ? " is-selected" : "")
            }
            onClick={() => pick(d)}
          >
            {decisionLabel(d)}
            {d === "action" && <span className="mdb-btn__hint">+ disable place</span>}
          </button>
        ))}
      </div>

      {closing && (
        <label className="mdb__field">
          <span className="mdb__label">Resolution</span>
          <select
            className="mdb__select"
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

      <label className="mdb__field">
        <span className="mdb__label">
          Resolution note {closing ? "" : "(reopen reason)"}
        </span>
        <textarea
          className="mdb__note"
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
        <label className="mdb__toggle">
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
        <p className="mdb__error" role="alert">
          Commit failed: {error}. Please try again.
        </p>
      )}

      <p className="mdb__contract">
        Commits <code>PATCH /places/api/reports/{card.id}</code>
        {disablePlace && canDisable ? (
          <>
            {" + "}
            <code>PATCH /places/api/places/{card.entityId}/disable</code>
          </>
        ) : null}{" "}
        <em>(admin-bearer gated &#x2014; fails closed 403 without a bearer)</em>.
      </p>

      <div className="mdb__actions">
        <Button variant="secondary" className="mdb-btn--cancel" onClick={onCancel}>
          Cancel
        </Button>
        <Button
          variant="primary"
          className={"mdb-btn--confirm" + (decision === "action" ? " mdb-btn--danger" : "")}
          onClick={onConfirm}
        >
          Confirm {decisionLabel(decision).toLowerCase()}
        </Button>
      </div>
    </div>
  );
}
