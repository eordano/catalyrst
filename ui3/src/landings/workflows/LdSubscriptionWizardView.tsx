import SitesChrome from "../../web/frames/SitesChrome";
import AcSignIn from "../../account/workflows/AcSignIn";
import AcNotificationGroupSettings from "../../account/pages/AcNotificationGroupSettings";
import "./ldsubscriptionwizardview.css";

export type LdSubscriptionGroup = {
  key: string;
  label: string;
  flag?: string;
  types: string[];
};

type LdSubscriptionWizardViewProps = {
  step?: string;
  value?: string;
  email?: string;
  emailConfirmed?: boolean;
  selection?: Record<string, boolean>;
  groups?: LdSubscriptionGroup[];
  enabledCount?: number;
  error?: string;
  lastKind?: "subscribe" | "unsubscribe";
  onStart?: () => void;
  onSignIn?: () => void;
  onToggle?: (notificationType: string, enabled: boolean) => void;
  onSubmit?: () => void;
  onEdit?: () => void;
  onUnsubscribe?: () => void;
  onResubscribe?: () => void;
  onRetry?: () => void;
  onBack?: () => void;
};

export default function LdSubscriptionWizardView({
  step = "idle",
  value = "idle",
  email = "",
  emailConfirmed = false,
  selection = {},
  groups = [],
  enabledCount = 0,
  error = undefined,
  lastKind = "subscribe",
  onStart = undefined,
  onSignIn = undefined,
  onToggle = undefined,
  onSubmit = undefined,
  onEdit = undefined,
  onUnsubscribe = undefined,
  onResubscribe = undefined,
  onRetry = undefined,
  onBack = undefined,
}: LdSubscriptionWizardViewProps) {
  return (
    <SitesChrome active="whatson" signedIn={value !== "idle" && value !== "signinGate"}>
      <div className="subscription-wizard" data-step={step}>
        {value === "idle" && (
          <section className="subscription-wizard__intro">
            <h1>Never miss an event</h1>
            <p>
              Subscribe to email notifications and we&apos;ll let you know when an
              event you care about is starting in Decentraland.
            </p>
            <div className="subscription-wizard__controls">
              <button
                type="button"
                className="subscription-wizard__btn subscription-wizard__btn--primary"
                onClick={onStart}
              >
                Subscribe to event notifications
              </button>
            </div>
          </section>
        )}

        {value === "signinGate" && (
          <>
            <AcSignIn />
            <div className="subscription-wizard__controls" role="group" aria-label="Sign in">
              <button
                type="button"
                className="subscription-wizard__btn subscription-wizard__btn--primary"
                onClick={onSignIn}
              >
                I&apos;m signed in &#x2014; continue
              </button>
            </div>
          </>
        )}

        {(value === "editing" || value === "subscribed") && (
          <>
            <AcNotificationGroupSettings email={email} hasEmail={emailConfirmed} />

            <fieldset className="subscription-wizard__select" aria-label="Email notification types">
              <legend>Choose what we email you ({enabledCount} on)</legend>
              {groups.map((group) => (
                <div className="subscription-wizard__group" key={group.key} data-group={group.key}>
                  <span className="subscription-wizard__group-label">{group.label}</span>
                  {group.types.map((nt) => (
                    <label className="subscription-wizard__type" key={nt}>
                      <input
                        type="checkbox"
                        checked={!!selection[nt]}
                        onChange={(e) => onToggle?.(nt, e.currentTarget.checked)}
                      />
                      {nt}
                    </label>
                  ))}
                </div>
              ))}
            </fieldset>

            <div className="subscription-wizard__controls">
              {value === "editing" ? (
                <button
                  type="button"
                  className="subscription-wizard__btn subscription-wizard__btn--primary"
                  onClick={onSubmit}
                >
                  Save &amp; subscribe
                </button>
              ) : (
                <>
                  <span className="subscription-wizard__badge" role="status">
                    Subscribed
                  </span>
                  <button
                    type="button"
                    className="subscription-wizard__btn"
                    onClick={onEdit}
                  >
                    Edit selection
                  </button>
                  <button
                    type="button"
                    className="subscription-wizard__btn subscription-wizard__btn--danger"
                    onClick={onUnsubscribe}
                  >
                    Unsubscribe
                  </button>
                </>
              )}
            </div>
          </>
        )}

        {value === "submitting" && (
          <section className="subscription-wizard__progress" role="status" aria-busy="true">
            <h1>Saving your subscription&#x2026;</h1>
            <p>Updating your email notification preferences.</p>
          </section>
        )}

        {value === "unsubscribing" && (
          <section className="subscription-wizard__progress" role="status" aria-busy="true">
            <h1>Unsubscribing&#x2026;</h1>
            <p>Turning off your event email notifications.</p>
          </section>
        )}

        {value === "unsubscribed" && (
          <section className="subscription-wizard__done">
            <h1>You&apos;re unsubscribed</h1>
            <p>You will no longer receive event email notifications.</p>
            <div className="subscription-wizard__controls">
              <button
                type="button"
                className="subscription-wizard__btn subscription-wizard__btn--primary"
                onClick={onResubscribe}
              >
                Re-subscribe
              </button>
            </div>
          </section>
        )}

        {value === "error" && (
          <section className="subscription-wizard__error" role="alert">
            <h1>Something went wrong</h1>
            <p>
              We couldn&apos;t {lastKind === "unsubscribe" ? "unsubscribe" : "save your subscription"}
              {error ? ` (${error})` : ""}. Please try
              again.
            </p>
            <div className="subscription-wizard__controls">
              <button
                type="button"
                className="subscription-wizard__btn subscription-wizard__btn--primary"
                onClick={onRetry}
              >
                Retry
              </button>
              <button
                type="button"
                className="subscription-wizard__btn"
                onClick={onBack}
              >
                Back to settings
              </button>
            </div>
          </section>
        )}
      </div>
    </SitesChrome>
  );
}
