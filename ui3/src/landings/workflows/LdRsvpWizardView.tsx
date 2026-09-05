import Button from "../../atoms/Button";
import EventDetail from "../../explorer/pages/EventDetail";
import "./ldrsvpwizardview.css";

export type LdRsvpEventInfo = {
  title: string;
  when: string;
  host: string;
  description: string;
  schedule: string;
  location: string;
  jumpHref: string;
};

const EMPTY_EVENT: LdRsvpEventInfo = {
  title: "",
  when: "",
  host: "",
  description: "",
  schedule: "",
  location: "",
  jumpHref: "",
};

type LdRsvpWizardViewProps = {
  step?: string;
  value?: string;
  event?: LdRsvpEventInfo;
  count?: number;
  error?: string;
  onTapGoing?: () => void;
  onCancel?: () => void;
  onSignIn?: () => void;
  onBack?: () => void;
  onConfirm?: () => void;
  onCancelRsvp?: () => void;
  onDismiss?: () => void;
  onRetry?: () => void;
};

export default function LdRsvpWizardView({
  step = "idle",
  value = "idle",
  event = EMPTY_EVENT,
  count = 0,
  error = undefined,
  onTapGoing = undefined,
  onCancel = undefined,
  onSignIn = undefined,
  onBack = undefined,
  onConfirm = undefined,
  onCancelRsvp = undefined,
  onDismiss = undefined,
  onRetry = undefined,
}: LdRsvpWizardViewProps) {
  return (
    <div className="rsvp-wizard" data-step={step}>
      <EventDetail
        event={{
          title: event.title,
          when: event.when,
          host: event.host,
          description: event.description,
          schedule: event.schedule,
          location: event.location,
        }}
        jumpHref={event.jumpHref}
      />

      <div className="rsvp-wizard__panel" role="group" aria-label="RSVP to this event">
        <div className="rsvp-wizard__count" aria-live="polite">
          <strong>{count}</strong> going
        </div>

        {value === "idle" && (
          <Button onClick={onTapGoing}>Going</Button>
        )}

        {value === "signinGate" && (
          <>
            <p className="rsvp-wizard__hint">
              Sign in to RSVP. Your wallet signs the request.
            </p>
            <div className="rsvp-wizard__row">
              <Button variant="secondary" onClick={onCancel}>
                Cancel
              </Button>
              <Button onClick={onSignIn}>Sign in</Button>
            </div>
          </>
        )}

        {value === "confirming" && (
          <>
            <p className="rsvp-wizard__hint">
              Confirm you&apos;re going to <b>{event.title}</b>. You can cancel
              your RSVP anytime.
            </p>
            <div className="rsvp-wizard__row">
              <Button variant="secondary" onClick={onBack}>
                Back
              </Button>
              <Button onClick={onConfirm}>Confirm RSVP</Button>
            </div>
          </>
        )}

        {value === "submitting" && (
          <p className="rsvp-wizard__hint" aria-busy="true">
            Submitting your RSVP&#x2026;
          </p>
        )}

        {value === "going" && (
          <>
            <p className="rsvp-wizard__hint rsvp-wizard__hint--ok">
              You&apos;re going! See you there.
            </p>
            <Button variant="secondary" onClick={onCancelRsvp}>
              Cancel RSVP
            </Button>
          </>
        )}

        {value === "cancelling" && (
          <p className="rsvp-wizard__hint" aria-busy="true">
            Cancelling your RSVP&#x2026;
          </p>
        )}

        {value === "notGoing" && (
          <>
            <p className="rsvp-wizard__hint">Your RSVP was cancelled.</p>
            <Button onClick={onTapGoing}>Going again</Button>
          </>
        )}

        {value === "error" && (
          <>
            <p className="rsvp-wizard__hint rsvp-wizard__hint--err" role="alert">
              RSVP failed{error ? `: ${error}` : ""}.
            </p>
            <div className="rsvp-wizard__row">
              <Button variant="secondary" onClick={onDismiss}>
                Dismiss
              </Button>
              <Button onClick={onRetry}>Retry</Button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
