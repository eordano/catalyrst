import Button from "../../atoms/Button";
import TeleportPrompt from "./TeleportPrompt";
import "./jumpinview.css";

export type JumpInViewProps = {
  variant: string;
  placeTitle: string;
  launching: boolean;
  confirming: boolean;
  launched: boolean;
  errored: boolean;
  error?: string;
  onStart: () => void;
  onCancel: () => void;
  onConfirm: () => void;
  onRetry: () => void;
};

export default function JumpInView({
  variant,
  placeTitle,
  launching,
  confirming,
  launched,
  errored,
  error,
  onStart,
  onCancel,
  onConfirm,
  onRetry,
}: JumpInViewProps) {
  return (
    <div className="jumpin" data-variant={variant}>
      <Button
        variant="primary"
        className="jumpin__cta"
        disabled={launching}
        aria-busy={launching || undefined}
        onClick={onStart}
      >
        {launching ? "LAUNCHING\u{2026}" : "JUMP IN"}
      </Button>

      {confirming && (
        <div className="jumpin__confirm" role="presentation">
          <TeleportPrompt showActions={false} />
          <div className="jumpin__confirm-actions">
            <Button
              variant="ghost"
              className="jumpin__cancel"
              onClick={onCancel}
            >
              CANCEL
            </Button>
            <Button
              variant="primary"
              className="jumpin__confirm-jump"
              onClick={onConfirm}
            >
              JUMP IN
            </Button>
          </div>
        </div>
      )}

      {launched && (
        <p className="jumpin__status" role="status">
          Launching {placeTitle}&#x2026;
        </p>
      )}

      {errored && (
        <div className="jumpin__error" role="alert">
          <p>Couldn&#x2019;t start the jump: {error ?? "unknown error"}</p>
          <Button
            variant="secondary"
            className="jumpin__retry"
            onClick={onRetry}
          >
            RETRY
          </Button>
        </div>
      )}
    </div>
  );
}
