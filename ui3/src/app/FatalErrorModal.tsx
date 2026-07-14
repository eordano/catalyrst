import { useEffect, useRef } from "react";
import Button from "../atoms/Button";
import "./fatalerror.css";

type FatalErrorModalProps = {
  message: string;
  onReload: () => void;
};

export default function FatalErrorModal({ message, onReload }: FatalErrorModalProps) {
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    cardRef.current?.focus();
  }, []);

  return (
    <div className="fatal__backdrop">
      <div
        className="fatal__card"
        role="alertdialog"
        aria-modal="true"
        aria-label="Something went wrong"
        tabIndex={-1}
        ref={cardRef}
      >
        <div className="fatal__head">
          <h2 className="fatal__title">Something went wrong</h2>
          <p className="fatal__subtitle">The app hit an unexpected error.</p>
        </div>

        <pre className="fatal__detail">{message}</pre>

        <div className="fatal__actions">
          <Button variant="primary" className="fatal__btn" onClick={onReload}>
            Reload
          </Button>
        </div>
      </div>
    </div>
  );
}
