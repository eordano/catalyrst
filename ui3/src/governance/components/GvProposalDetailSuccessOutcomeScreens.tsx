import { useState } from "react";
import Modal from "../../components/Modal";
import { Close } from "../../atoms/icons";
import "./gvproposaldetailsuccessoutcomescreens.css";

const SUCCESS_SHARED = {
  sub: "Here's what you could do next:",
  forumTitle: "Join the discussion",
  forumDesc: "Comment, ideate, expand.",
  forumLabel: "View on the forum",
  discordTitle: "Discuss on Discord",
  discordDesc: "Engage with the Community on our #dao channel",
  discordLabel: "Join Discord",
  copyTitle: "Share it on Social Media",
  copyDesc: "Gather feedback, ideate, and improve",
  copyLabel: "Copy Link",
  copiedLabel: "Link Copied!",
  dismissLabel: "Dismiss",
};

const PENDING_SHARED = {
  addToCalendar: "Add to calendar",
  votingBegins: (date: string) => `Voting begins: ${date}`,
};

type SuccessVariant = {
  kind: "success";
  title: string;
  description: string;
  hasLink: boolean;
};

type PendingVariant = {
  kind: "pending";
  title: string;
  description: string;
  helper: string;
  votingStartsAt: string;
  calendarUrl: string;
};

type VariantData = SuccessVariant | PendingVariant;

type VariantName = "new" | "update" | "pending" | "bid";

const VARIANTS: Record<VariantName, VariantData> = {
  new: {
    kind: "success",
    title: "Proposal successfully submitted",
    description: "Thanks for taking part in the DAO",
    hasLink: true,
  },
  update: {
    kind: "success",
    title: "Thanks for sharing your Grant Update",
    description: "Now go make some noise and let the\n\ncommunity know about your progress.",
    hasLink: true,
  },
  pending: {
    kind: "pending",
    title: "Proposal published",
    description:
      "Your proposal is now published. When the Tender submission period ends, it will be open for voting.",
    helper: "Set a reminder in your Calendar for when the voting period begins.",
    votingStartsAt: "Jul 18, 2026 14:00",
    calendarUrl: "https://www.google.com/calendar/render?action=TEMPLATE",
  },
  bid: {
    kind: "pending",
    title: "Bid submitted but not published",
    description:
      "Your bid has been successfully submitted and stored. It will remain private until the voting period starts. It will only be published for voting if at least one competing bid for the same Tender exists.",
    helper: "Set a reminder in your Calendar for when the voting period begins.",
    votingStartsAt: "Jul 25, 2026 09:30",
    calendarUrl: "https://www.google.com/calendar/render?action=TEMPLATE",
  },
};

function Hourglass() {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="108" height="108" fill="none" viewBox="0 0 108 108" aria-hidden="true">
      <circle cx="54" cy="54" r="54" fill="#FEF7D2" />
      <path
        fill="#FFB03F"
        d="M72.313 27C73.802 27 75 28.128 75 29.531c0 1.403-1.198 2.532-2.688 2.532h-.895v2.003c0 4.25-1.792 8.332-4.983 11.338L57.296 54l9.138 8.596c3.191 3.006 4.983 7.087 4.983 11.338v2.004h.895c1.49 0 2.688 1.128 2.688 2.53C75 79.872 73.802 81 72.312 81H34.688C33.197 81 32 79.871 32 78.469c0-1.403 1.198-2.531 2.688-2.531h.895v-2.004c0-4.25 1.792-8.332 4.983-11.338L49.704 54l-9.126-8.596c-3.203-3.006-4.995-7.087-4.995-11.338v-2.004h-.895c-1.49 0-2.688-1.128-2.688-2.53C32 28.127 33.198 27 34.688 27h37.624zM53.5 57.575l-9.126 8.596c-2.184 2.067-3.416 4.852-3.416 7.763v-5.71h25.084v5.71c0-2.911-1.232-5.696-3.416-7.752L53.5 57.575zm0-15.757H62.626c2.184-2.056 3.416-4.84 3.416-7.752v-2.004H40.958v2.004c0 2.911 1.232 5.696 3.416 7.752H53.5z"
      />
    </svg>
  );
}

type SuccessCardProps = {
  data: SuccessVariant;
  loading: boolean;
  onDismiss: () => void;
};

function SuccessCard({ data, loading, onDismiss }: SuccessCardProps) {
  const [copied, setCopied] = useState(false);
  const s = SUCCESS_SHARED;
  return (
    <>
      <div className="gpsuc__title">
        <h2 className="gpsuc__heading">{data.title}</h2>
        <p className="gpsuc__lead">{data.description}</p>
        <p className="gpsuc__sub">{s.sub}</p>
      </div>

      {data.hasLink && (
        <div className="gpsuc__banner gpsuc__banner--forum">
          <div className="gpsuc__banner-desc">
            <p className="gpsuc__banner-title">{s.forumTitle}</p>
            <p className="gpsuc__banner-text">{s.forumDesc}</p>
          </div>
          <a
            className="gpsuc__banner-btn gpsuc__banner-btn--forum"
            href="https://forum.decentraland.org/"
            target="_blank"
            rel="noreferrer"
          >
            {s.forumLabel}
          </a>
        </div>
      )}

      <div className="gpsuc__banner gpsuc__banner--discord">
        <div className="gpsuc__banner-desc">
          <p className="gpsuc__banner-title">{s.discordTitle}</p>
          <p className="gpsuc__banner-text">{s.discordDesc}</p>
        </div>
        <a
          className="gpsuc__banner-btn gpsuc__banner-btn--discord"
          href="https://decentraland.org/discord"
          target="_blank"
          rel="noreferrer"
        >
          {s.discordLabel}
        </a>
      </div>

      {data.hasLink && (
        <div className="gpsuc__banner gpsuc__banner--copy">
          <div className="gpsuc__banner-desc">
            <p className="gpsuc__banner-title">{s.copyTitle}</p>
            <p className="gpsuc__banner-text">{s.copyDesc}</p>
          </div>
          <button
            type="button"
            className="gpsuc__banner-btn gpsuc__banner-btn--copy"
            onClick={() => setCopied(true)}
          >
            {copied ? s.copiedLabel : s.copyLabel}
          </button>
        </div>
      )}

      <div className="gpsuc__actions">
        <button type="button" className="gpsuc__dismiss" onClick={onDismiss} disabled={loading}>
          {s.dismissLabel}
        </button>
      </div>
    </>
  );
}

type PendingCardProps = {
  data: PendingVariant;
};

function PendingCard({ data }: PendingCardProps) {
  const hasCalendar = !!data.calendarUrl;
  return (
    <div className="gpsuc__pending">
      <div className="gpsuc__pending-icon">
        <Hourglass />
      </div>
      <h3 className="gpsuc__pending-title">{data.title}</h3>
      <p className="gpsuc__pending-desc">{data.description}</p>
      {data.votingStartsAt && (
        <p className="gpsuc__pending-desc">{PENDING_SHARED.votingBegins(data.votingStartsAt)}</p>
      )}
      <a
        className="gpsuc__pending-btn"
        href={hasCalendar ? data.calendarUrl : undefined}
        target="_blank"
        rel="noreferrer"
        aria-disabled={hasCalendar ? undefined : "true"}
      >
        {PENDING_SHARED.addToCalendar}
      </a>
      <p className="gpsuc__pending-helper">{data.helper}</p>
    </div>
  );
}

type GvProposalDetailSuccessOutcomeScreensProps = {
  variant?: VariantName;
  loading?: boolean;
  onClose?: () => void;
  onDismiss?: () => void;
};

export default function GvProposalDetailSuccessOutcomeScreens({
  variant = "new",
  loading = false,
  onClose = () => {},
  onDismiss = () => {},
}: GvProposalDetailSuccessOutcomeScreensProps) {
  const data = VARIANTS[variant] || VARIANTS.new;

  return (
    <Modal onClose={onClose} width={360} className="modal__card--plain gpsuc__card" ariaLabel={data.title}>
        <button type="button" className="gpsuc__close" aria-label="Close" onClick={onClose}>
          <Close size={18} />
        </button>

        {data.kind === "success" ? (
          <SuccessCard data={data} loading={loading} onDismiss={onDismiss} />
        ) : (
          <PendingCard data={data} />
        )}
    </Modal>
  );
}
