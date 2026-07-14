import { useEffect, useMemo, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import StWhatSOnAdminPendingEvents from "@ui/web/pages/StWhatSOnAdminPendingEvents";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  type AdminEventCard,
  type ModerationAction,
  type RejectReason,
  MAX_REJECTION_REASON_LENGTH,
} from "@data/lib/catalyst/admin/whatson-admin";
import {
  moderateMachine,
  resolveModerateSnapshot,
  slugToState,
  stateToSlug,
  type ModerateStateId,
  type ModerateFn,
  type TrackFn,
} from "./machine";

export type ModerateWizardProps = {
  trackCtx: TrackContext;
  pending: AdminEventCard[];
  approved: AdminEventCard[];
  featured: AdminEventCard[];
  rejectReasons: RejectReason[];
  initialStep?: string;
  moderate?: ModerateFn;
  track?: TrackFn;
};

export default function ModerateWizard({
  trackCtx,
  pending,
  approved,
  featured,
  rejectReasons,
  initialStep,
  moderate,
  track,
}: ModerateWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <ModerateWizardInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      pending={pending}
      approved={approved}
      featured={featured}
      rejectReasons={rejectReasons}
      moderate={moderate}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ModerateStateId;
  trackCtx: TrackContext;
  pending: AdminEventCard[];
  approved: AdminEventCard[];
  featured: AdminEventCard[];
  rejectReasons: RejectReason[];
  moderate?: ModerateFn;
  track?: TrackFn;
};

function ModerateWizardInner({
  stateId,
  trackCtx,
  pending,
  approved,
  featured,
  rejectReasons,
  moderate,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const seedEventId = pending[0]?.id ?? approved[0]?.id ?? "sample-event";

  const snapshot = useRef(
    resolveModerateSnapshot({
      step: stateId,
      trackCtx,
      moderate,
      track,
      eventId: seedEventId,
    }),
  ).current;

  const [state, send] = useMachine(moderateMachine, {
    input: { trackCtx, moderate, track },
    snapshot,
  });

  const value = state.value as ModerateStateId;
  const step = stateToSlug(value);

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  const allCards = useMemo(
    () => [...pending, ...approved, ...featured],
    [pending, approved, featured],
  );
  const activeId = state.context.eventId;
  const activeCard =
    allCards.find((c) => c.id === activeId) ??
    (activeId ? { id: activeId, name: activeId, creator: "\u{2014}", time: "--:--", dateLabel: "SOON", hue: 280, image: null, description: "", highlighted: false } : undefined);

  const gated = value === "authGate";

  return (
    <div className="moderate-wizard" data-step={step} data-state={value}>
      <StWhatSOnAdminPendingEvents
        pending={pending}
        approved={approved}
        allowed={!gated}
        loading={false}
      />

      <div className="mw-layer" role="group" aria-label="Moderation wizard">
        {value === "authGate" && (
          <div className="mw-panel mw-panel--gate" role="region" aria-label="Admin sign in">
            <h2 className="mw-panel__title">Admin sign-in required</h2>
            <p className="mw-panel__text">
              Moderating What&apos;s On hangouts requires an admin bearer token.
              <br />
              <em>(The admin bearer is simulated for this preview.)</em>
            </p>
            <button
              type="button"
              className="mw-btn mw-btn--primary"
              onClick={() => send({ type: "SIGN_IN" })}
            >
              Sign in as admin (simulated)
            </button>
          </div>
        )}

        {value === "queue" && (
          <div className="mw-panel mw-panel--queue">
            <h2 className="mw-panel__title">Review queue</h2>
            {pending.length === 0 ? (
              <p className="mw-panel__text">No hangouts waiting for approval.</p>
            ) : (
              <ul className="mw-queuelist">
                {pending.map((card) => (
                  <li key={card.id} className="mw-queueitem">
                    <span className="mw-queueitem__name">{card.name}</span>
                    <span className="mw-queueitem__meta">
                      {card.dateLabel} &#xB7; {card.time} &#xB7; by {card.creator}
                    </span>
                    <button
                      type="button"
                      className="mw-btn"
                      onClick={() => send({ type: "OPEN", eventId: card.id })}
                    >
                      Review
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        {value === "reviewEvent" && activeCard && (
          <ReviewPanel
            card={activeCard}
            onDecide={(action) => send({ type: "DECIDE", action })}
            onClose={() => send({ type: "CLOSE" })}
          />
        )}

        {value === "decision" && activeCard && (
          <DecisionPanel
            card={activeCard}
            action={state.context.action ?? "approve"}
            rejectReasons={rejectReasons}
            error={state.context.error}
            onReDecide={(action, reasons, note) =>
              send({ type: "DECIDE", action, rejectReasons: reasons, rejectNote: note })
            }
            onConfirm={() => send({ type: "CONFIRM" })}
            onCancel={() => send({ type: "CANCEL" })}
          />
        )}

        {value === "submitting" && (
          <div className="mw-panel mw-panel--submitting" role="status">
            <span className="mw-spinner" aria-hidden="true" />
            <p className="mw-panel__text">
              Applying moderation&#x2026; <em>(simulated PATCH)</em>
            </p>
          </div>
        )}

        {value === "moderated" && activeCard && (
          <div className="mw-panel mw-panel--done" role="status" aria-live="polite">
            <h2 className="mw-panel__title">Moderation applied</h2>
            <p className="mw-panel__text">
              <strong>{activeCard.name}</strong> &middot;{" "}
              {actionLabel(state.context.action ?? "approve")} (simulated).
            </p>
            <button
              type="button"
              className="mw-btn mw-btn--primary"
              onClick={() => send({ type: "CONTINUE" })}
            >
              Continue to next
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function actionLabel(action: ModerationAction): string {
  switch (action) {
    case "approve":
      return "Approved";
    case "reject":
      return "Rejected";
    case "feature":
      return "Featured";
    case "unfeature":
      return "Unfeatured";
    case "archive":
      return "Archived";
  }
}

function ReviewPanel({
  card,
  onDecide,
  onClose,
}: {
  card: AdminEventCard;
  onDecide: (action: ModerationAction) => void;
  onClose: () => void;
}) {
  return (
    <div className="mw-panel mw-panel--review" role="region" aria-label={`Review ${card.name}`}>
      <div
        className="mw-review__hero"
        style={{ "--mw-hue": String(card.hue) } as React.CSSProperties}
        role="img"
        aria-label={card.name}
      />
      <h2 className="mw-panel__title">{card.name}</h2>
      <p className="mw-review__meta">
        {card.dateLabel} &#xB7; {card.time} &#xB7; by {card.creator}
      </p>
      {card.description && <p className="mw-review__desc">{card.description}</p>}
      <div className="mw-actions">
        <button type="button" className="mw-btn" onClick={onClose}>
          Back to queue
        </button>
        <button
          type="button"
          className="mw-btn mw-btn--reject"
          onClick={() => onDecide("reject")}
        >
          Reject
        </button>
        <button
          type="button"
          className="mw-btn"
          onClick={() => onDecide(card.highlighted ? "unfeature" : "feature")}
        >
          {card.highlighted ? "Unfeature" : "Feature"}
        </button>
        <button
          type="button"
          className="mw-btn mw-btn--approve"
          onClick={() => onDecide("approve")}
        >
          Approve
        </button>
      </div>
    </div>
  );
}

function DecisionPanel({
  card,
  action,
  rejectReasons,
  error,
  onReDecide,
  onConfirm,
  onCancel,
}: {
  card: AdminEventCard;
  action: ModerationAction;
  rejectReasons: RejectReason[];
  error?: string;
  onReDecide: (action: ModerationAction, reasons?: string[], note?: string) => void;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const [reasons, setReasons] = useState<Set<string>>(new Set());
  const [note, setNote] = useState("");
  const [reasonError, setReasonError] = useState(false);

  const isReject = action === "reject";

  function toggleReason(code: string) {
    setReasonError(false);
    setReasons((prev) => {
      const next = new Set(prev);
      if (next.has(code)) next.delete(code);
      else next.add(code);
      onReDecide("reject", Array.from(next), note.trim());
      return next;
    });
  }

  function confirm() {
    if (isReject && reasons.size === 0) {
      setReasonError(true);
      return;
    }
    onConfirm();
  }

  return (
    <div className="mw-panel mw-panel--decision" role="region" aria-label="Confirm moderation">
      <h2 className="mw-panel__title">
        {actionLabel(action)} &#x201C;{card.name}&#x201D;?
      </h2>

      {isReject && (
        <div className="mw-reject">
          <p className="mw-reject__title">Reject reason*</p>
          <div className="mw-reject__list">
            {rejectReasons.map((r) => {
              const on = reasons.has(r.code);
              return (
                <label key={r.code} className="mw-reject__row">
                  <input
                    type="checkbox"
                    checked={on}
                    onChange={() => toggleReason(r.code)}
                    aria-label={r.title}
                  />
                  <span>
                    <strong>{r.title}</strong> &#x2014; {r.description}
                  </span>
                </label>
              );
            })}
          </div>
          {reasonError && (
            <p className="mw-reject__error" role="alert">
              Select at least one reason
            </p>
          )}
          <label className="mw-reject__notelabel">
            Other (optional)
            <textarea
              className="mw-reject__note"
              rows={2}
              maxLength={MAX_REJECTION_REASON_LENGTH}
              placeholder="User will receive a notification, be as descriptive as you can."
              value={note}
              onChange={(e) => {
                setNote(e.target.value);
                onReDecide("reject", Array.from(reasons), e.target.value.trim());
              }}
            />
          </label>
        </div>
      )}

      {!isReject && (
        <p className="mw-panel__text">
          This will {actionLabel(action).toLowerCase()} the hangout via{" "}
          <code>PATCH /events/api/events/{card.id}</code> (admin bearer simulated).
        </p>
      )}

      {error && (
        <p className="mw-panel__error" role="alert">
          Moderation failed: {error}. Please try again.
        </p>
      )}

      <div className="mw-actions">
        <button type="button" className="mw-btn" onClick={onCancel}>
          Cancel
        </button>
        <button
          type="button"
          className={"mw-btn mw-btn--primary" + (isReject ? " mw-btn--reject" : "")}
          onClick={confirm}
        >
          Confirm {actionLabel(action).toLowerCase()}
        </button>
      </div>
    </div>
  );
}
