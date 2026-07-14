import type { ReactNode } from "react";
import { useId, useRef } from "react";
import { useDialogKeys } from "../../components/useDialogKeys";
import "./confirmdialog.css";

type ConfirmDialogProps = {
  title?: ReactNode;
  body?: ReactNode;
  confirmLabel?: ReactNode;
  cancelLabel?: ReactNode;
  danger?: boolean;
  variant?: "default" | "gradient";
  gradient?: string;
  avatar?: ReactNode;
  confirmTone?: string;
  onConfirm?: () => void;
  onCancel?: () => void;
};

export default function ConfirmDialog({
  title, body, confirmLabel = "Yes", cancelLabel = "No", danger = false,
  variant = "default",
  gradient = "teal",
  avatar,
  confirmTone,
  onConfirm, onCancel,
}: ConfirmDialogProps) {
  const cardRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const bodyId = useId();

  useDialogKeys(cardRef, onCancel);

  const isGradient = variant === "gradient";
  const cardCls =
    "cfd" +
    (isGradient ? " cfd--gradient cfd--grad-" + gradient : "") +
    (isGradient && avatar ? " cfd--has-top" : "");

  const tone = confirmTone || (danger ? "danger" : "");

  return (
    <div className="cfd__scrim" onClick={onCancel}>
      <div
        className={cardCls}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={title ? titleId : undefined}
        aria-describedby={body ? bodyId : undefined}
        aria-label={title ? undefined : "Confirm"}
        tabIndex={-1}
        ref={cardRef}
        onClick={(e) => e.stopPropagation()}
      >
        {isGradient && avatar && (
          typeof avatar === "string"
            ? <img className="cfd__avatar" src={avatar} alt="" aria-hidden="true" />
            : <div className="cfd__top" aria-hidden="true">{avatar}</div>
        )}
        {title && <h2 className="cfd__title" id={titleId}>{title}</h2>}
        {body && <p className="cfd__body" id={bodyId}>{body}</p>}
        <div className="cfd__actions">
          <button className="cfd__cancel" onClick={onCancel}>{cancelLabel}</button>
          <button className={"cfd__confirm" + (tone ? " is-" + tone : "")} onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
