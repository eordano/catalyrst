import GvAccountIdentityLinkingFlow from "./GvAccountIdentityLinkingFlow";
import "./gvlinkaccountsview.css";

const STEP_LABELS: { slug: string; label: string }[] = [
  { slug: "choose", label: "Choose" },
  { slug: "connect", label: "Connect" },
  { slug: "verifying", label: "Verifying" },
  { slug: "connected", label: "Connected" },
];

function subScreenFor(value: string, account: string): string {
  switch (value) {
    case "choosing":
      return "choose";
    case "connecting":
      return account === "push" ? "push" : account;
    case "verifying":
      return "push";
    case "connected":
      return "post-success";
    case "error":
      return "post-error";
    case "unlinkConfirm":
      return "unlink-confirm";
    case "unlinking":
      return "unlink-row";
    default:
      return "choose";
  }
}

type GvLinkAccountCopy = {
  kind: string;
  card_title: string;
  title: string;
  confirm_label: string;
  success_text: string;
  error_button: string;
};

type GvLinkAccountsViewProps = {
  value?: string;
  step?: string;
  account?: string;
  connectStep?: number;
  totalSteps?: number;
  title?: string;
  providers?: string[];
  accounts?: Record<string, GvLinkAccountCopy>;
  unlink?: { body: string; cancel_button: string; confirm_button: string; note: string };
  onChoose?: (provider: string) => void;
  onUnlinkRequest?: (account: string) => void;
  onNextStep?: () => void;
  onConfirm?: () => void;
  onCancel?: () => void;
  onConfirmUnlink?: () => void;
  onBack?: () => void;
  onRetry?: () => void;
};

const FALLBACK_COPY: GvLinkAccountCopy = {
  kind: "steps",
  card_title: "",
  title: "",
  confirm_label: "",
  success_text: "",
  error_button: "Retry",
};

export default function GvLinkAccountsView({
  value = "choosing",
  step = "choose",
  account = "forum",
  connectStep = 1,
  totalSteps = 1,
  title = "",
  providers = [],
  accounts = {},
  unlink = { body: "", cancel_button: "", confirm_button: "", note: "" },
  onChoose = undefined,
  onUnlinkRequest = undefined,
  onNextStep = undefined,
  onConfirm = undefined,
  onCancel = undefined,
  onConfirmUnlink = undefined,
  onBack = undefined,
  onRetry = undefined,
}: GvLinkAccountsViewProps) {
  const subScreen = subScreenFor(value, account);
  const copy = accounts[account] ?? FALLBACK_COPY;
  const completed = STEP_LABELS.findIndex((s) => s.slug === step);

  const stepRail = (
    <div className="gv-link-wizard__steps" role="list" aria-label="Linking steps">
      {STEP_LABELS.map((s, i) => (
        <span
          key={s.slug}
          className="gv-link-wizard__pip"
          role="listitem"
          data-active={s.slug === step}
          data-done={i < completed}
        >
          {s.label}
        </span>
      ))}
    </div>
  );

  return (
    <div className="gv-link-wizard" data-step={step} data-account={account}>
      <GvAccountIdentityLinkingFlow key={subScreen} initial={subScreen} />

      {stepRail}

      {value === "choosing" && (
        <>
          <p className="gv-link-wizard__hint">
            {title}. Choose an account to connect, then walk the connection flow.
          </p>
          <div className="gv-link-wizard__chooser" role="group" aria-label="Choose account">
            {providers.map((p) => (
              <button
                key={p}
                type="button"
                className={
                  "gv-link-wizard__btn" +
                  (p === account ? " gv-link-wizard__btn--primary" : "")
                }
                onClick={() => onChoose?.(p)}
              >
                Connect {accounts[p]?.card_title}
              </button>
            ))}
          </div>
          <div className="gv-link-wizard__controls">
            <button
              type="button"
              className="gv-link-wizard__btn"
              onClick={() => onUnlinkRequest?.(account)}
            >
              Unlink {copy.card_title}
            </button>
          </div>
        </>
      )}

      {value === "connecting" && (
        <>
          <p className="gv-link-wizard__hint">
            {copy.title} &#x2014; step {connectStep} of {totalSteps}
            {copy.kind === "steps"
              ? " (Sign \u{2192} Copy \u{2192} Post)"
              : " (subscribe). The signature is simulated."}
          </p>
          <div className="gv-link-wizard__controls" role="group" aria-label="Connection controls">
            <button
              type="button"
              className="gv-link-wizard__btn"
              onClick={() => onBack?.()}
            >
              Back
            </button>
            {connectStep < totalSteps && (
              <button
                type="button"
                className="gv-link-wizard__btn"
                onClick={() => onNextStep?.()}
              >
                Next step
              </button>
            )}
            <button
              type="button"
              className="gv-link-wizard__btn gv-link-wizard__btn--primary"
              disabled={connectStep < totalSteps}
              onClick={() => onConfirm?.()}
            >
              {copy.confirm_label}
            </button>
          </div>
        </>
      )}

      {value === "verifying" && (
        <p className="gv-link-wizard__hint" aria-live="polite">
          Verifying your {copy.card_title} signature (simulated time-sensitive task).
        </p>
      )}

      {value === "connected" && (
        <>
          <p className="gv-link-wizard__hint" aria-live="polite">
            {strip(copy.success_text)}
          </p>
          <span className="gv-link-wizard__note">
            Linked in a simulation &#x2014; no on-chain transaction or backend write was made.
          </span>
        </>
      )}

      {value === "error" && (
        <div className="gv-link-wizard__controls" role="group" aria-label="Error controls">
          <button
            type="button"
            className="gv-link-wizard__btn"
            onClick={() => onBack?.()}
          >
            Back to steps
          </button>
          <button
            type="button"
            className="gv-link-wizard__btn gv-link-wizard__btn--primary"
            onClick={() => onRetry?.()}
          >
            {copy.error_button}
          </button>
        </div>
      )}

      {value === "unlinkConfirm" && (
        <>
          <p className="gv-link-wizard__hint">{unlink.body}</p>
          <div className="gv-link-wizard__controls" role="group" aria-label="Unlink controls">
            <button
              type="button"
              className="gv-link-wizard__btn"
              onClick={() => onCancel?.()}
            >
              {unlink.cancel_button}
            </button>
            <button
              type="button"
              className="gv-link-wizard__btn gv-link-wizard__btn--primary"
              onClick={() => onConfirmUnlink?.()}
            >
              {unlink.confirm_button}
            </button>
          </div>
          <span className="gv-link-wizard__note">{unlink.note}</span>
        </>
      )}

      {value === "unlinking" && (
        <p className="gv-link-wizard__hint" aria-live="polite">
          Unlinking {copy.card_title} (simulated)&#x2026;
        </p>
      )}
    </div>
  );
}

function strip(s: string): string {
  return s.replace(/\*\*/g, "");
}
