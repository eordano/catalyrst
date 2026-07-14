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
          The scene is <strong>running</strong>. Changes you make now are temporary &#x2014; they stay
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


// The MCP pairing consent surface. mcp-bridge owns the trust decision (loopback
// silent, remote refused without consent); this owns only its presentation, so
// it themes and traps focus like every other dialog.
export function McpPairingConsentModal({
  host,
  onAnswer,
}: {
  host: string;
  onAnswer: (approved: boolean) => void;
}) {
  return (
    <Modal
      onClose={() => onAnswer(false)}
      width={440}
      role="alertdialog"
      ariaLabel="Pair with a remote control server?"
      showClose={false}
    >
      <div className="eui-json-modal" data-mcp-pairing-gate="">
        <div style={{ fontWeight: 600, marginBottom: 8 }}>Pair with a remote control server?</div>
        <p style={{ margin: "0 0 8px" }}>
          This link asks to connect the editor to a control server, which lets it watch the
          screen, run the scene, and change the project.
        </p>
        <p style={{ margin: "0 0 8px", fontFamily: "var(--font-mono)", wordBreak: "break-all" }} data-mcp-pairing-host="">
          {host}
        </p>
        <p style={{ margin: 0, opacity: 0.8 }}>Only pair if you started that server yourself.</p>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 12 }}>
          <button className="eui-btn primary" type="button" autoFocus onClick={() => onAnswer(false)}>
            {"Don't pair"}
          </button>
          <button className="eui-btn" type="button" onClick={() => onAnswer(true)}>
            {`Pair with ${host}`}
          </button>
        </div>
      </div>
    </Modal>
  );
}
