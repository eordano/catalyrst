import { useCallback, useEffect, useState } from "react";
import { useBridgeState } from "../../overlay/bridge";
import Spinner from "../../atoms/Spinner";
import Modal from "../../components/Modal";
import "./logincode.css";

function openExternal(url: string | null | undefined) {
  if (url && typeof window !== "undefined")
    window.open(url, "_blank", "noopener,noreferrer");
}

export default function LoginCodeModal() {
  const loginCode = useBridgeState((s) => s.loginCode);
  const identity = useBridgeState((s) => s.identity);
  const [dismissed, setDismissed] = useState(false);

  const key = loginCode ? `${loginCode.code ?? ""}|${loginCode.error ?? ""}` : "";
  useEffect(() => {
    if (loginCode) setDismissed(false);
  }, [key]);

  // Stable callback: Modal's key-handling effect deps include onClose, and this component
  // is always mounted (it self-hides via `return null` below) — a fresh arrow function here
  // would re-run the effect on every unrelated app re-render, re-capturing and restoring
  // focus (e.g. stealing focus back from Chat's input right after the user closed it).
  const dismiss = useCallback(() => setDismissed(true), []);

  if (!loginCode || dismissed) return null;
  if (identity && identity.isGuest === false) return null;

  const { code, url, error } = loginCode;

  return (
    <Modal onClose={dismiss} ariaLabel="Sign in" className="lcm" showClose={false}>
      <h2 className="lcm__title">Sign in</h2>

      {error ? (
        <>
          <p className="lcm__body">Could not start the login: {error}</p>
          <div className="lcm__actions">
            <button className="lcm__close" onClick={() => setDismissed(true)}>
              Close
            </button>
          </div>
        </>
      ) : (
        <>
          <p className="lcm__body">
            Open the authorization page in your browser, confirm the code below
            matches, then sign with your wallet.
          </p>

          {code != null && (
            <>
              <div className="lcm__codelabel">VERIFICATION CODE</div>
              <div className="lcm__code">{code}</div>
            </>
          )}

          <div className="lcm__actions">
            <button
              className="lcm__open"
              disabled={!url}
              onClick={() => openExternal(url)}
            >
              Open authorization page
            </button>
          </div>

          {url && (
            <button
              type="button"
              className="lcm__link"
              onClick={() => openExternal(url)}
            >
              {url}
            </button>
          )}

          <div className="lcm__status" aria-live="polite">
            <Spinner size={13} color="#fff" aria-hidden />
            Waiting for authorization…
          </div>

          <button
            type="button"
            className="lcm__cancel"
            onClick={() => setDismissed(true)}
          >
            Cancel
          </button>
        </>
      )}
    </Modal>
  );
}
