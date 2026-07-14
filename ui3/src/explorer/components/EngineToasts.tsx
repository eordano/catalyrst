import type { BridgeToast } from "../../overlay/bridge";
import "./enginetoasts.css";

type EngineToastsProps = { toasts: BridgeToast[] };

export default function EngineToasts({ toasts }: EngineToastsProps) {
  if (toasts.length === 0) return null;
  return (
    <div className="etoast__stack" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.key} className="etoast">
          {t.message}
        </div>
      ))}
    </div>
  );
}
