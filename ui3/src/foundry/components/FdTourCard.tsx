import { useEffect } from "react";
import "./fdtourcard.css";

type FdTourCardProps = {
  step: number;
  total: number;
  title: string;
  body: string;
  onNext: () => void;
  onBack: () => void;
  onEnd: () => void;
};

export default function FdTourCard({
  step,
  total,
  title,
  body,
  onNext,
  onBack,
  onEnd,
}: FdTourCardProps) {
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") onEnd();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onEnd]);

  const last = step >= total;

  return (
    <aside
      className="fd-tour"
      role="dialog"
      aria-modal="false"
      aria-label={`Guided tour, step ${step} of ${total}`}
    >
      <p className="fd-tour__count">
        Step {step} of {total}
      </p>
      <h2 className="fd-tour__title">{title}</h2>
      <p className="fd-tour__body">{body}</p>

      <div className="fd-tour__actions">
        <button
          type="button"
          className="fd-tour__btn"
          onClick={onBack}
          disabled={step <= 1}
        >
          Back
        </button>
        <button type="button" className="fd-tour__btn fd-tour__btn--primary" onClick={onNext}>
          {last ? "Finish tour" : "Next"}
        </button>
        <button type="button" className="fd-tour__btn fd-tour__btn--quiet" onClick={onEnd}>
          End tour
        </button>
      </div>
    </aside>
  );
}
