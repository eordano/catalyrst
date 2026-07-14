import { useId, useState } from "react";
import Modal from "../../components/Modal";

export function PlayEditWarningModal({ onDismiss }: { onDismiss: (dontShowAgain: boolean) => void }) {
  const [dontShow, setDontShow] = useState(false);
  const checkboxId = useId();
  const close = () => onDismiss(dontShow);
  return (
    <Modal onClose={close} width={440} ariaLabel="Editing while the scene is running">
      <div className="eui-json-modal">
        <div style={{ fontWeight: 600, marginBottom: 8 }}>Editing while the scene is running</div>
        <p style={{ margin: "0 0 8px" }}>
          The scene is <strong>running</strong>. Changes you make now are temporary — they stay
          live in this run but <strong>won&rsquo;t be saved</strong>, and Stop restores the scene
          to how it was before Play.
        </p>
        <p style={{ margin: 0, opacity: 0.8 }}>Stop the scene to make changes that persist.</p>
        <label className="eui-check" htmlFor={checkboxId}>
          <input
            id={checkboxId}
            type="checkbox"
            checked={dontShow}
            onChange={(e) => setDontShow(e.target.checked)}
          />
          Don&rsquo;t show this again
        </label>
        <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 12 }}>
          <button className="eui-btn primary" type="button" onClick={close}>
            Got it
          </button>
        </div>
      </div>
    </Modal>
  );
}
