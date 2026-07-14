import { useEffect, useRef, useState } from "react";
import { useDialogKeys } from "./useDialogKeys";
import { fetchWorlds } from "../data/catalyst/placesSchema";
import "./worldvisitmodal.css";

type WorldVisitModalProps = {
  worldName: string;
  title?: string;
  onCancel: () => void;
  onConfirm: () => void;
};

export default function WorldVisitModal({ worldName, title, onCancel, onConfirm }: WorldVisitModalProps) {
  const [friendly, setFriendly] = useState(title ?? "");
  const cardRef = useRef<HTMLDivElement>(null);
  useDialogKeys(cardRef, onCancel);

  useEffect(() => {
    if (title) return;
    const ac = new AbortController();
    fetchWorlds({ names: worldName }, { signal: ac.signal })
      .then((rows) => setFriendly(rows[0]?.title ?? ""))
      .catch(() => {
      });
    return () => ac.abort();
  }, [worldName, title]);

  return (
    <div className="wvm__scrim" onClick={onCancel}>
      <div
        className="wvm"
        role="dialog"
        aria-modal="true"
        aria-label="Visit world"
        tabIndex={-1}
        ref={cardRef}
        onClick={(e) => e.stopPropagation()}
      >
        <button type="button" className="wvm__close" aria-label="Close" onClick={onCancel}>&#xD7;</button>
        <div className="wvm__question">Do you want to jump to the following realm?</div>
        <div className="wvm__realm">{worldName}</div>
        {friendly && friendly !== worldName && <div className="wvm__realmsub">{friendly}</div>}
        <div className="wvm__actions">
          <button type="button" className="wvm__cancel" onClick={onCancel}>CANCEL</button>
          <button type="button" className="wvm__go" onClick={onConfirm}>CONTINUE</button>
        </div>
      </div>
    </div>
  );
}
