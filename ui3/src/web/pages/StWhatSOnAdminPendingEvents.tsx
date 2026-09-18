import type { CSSProperties } from "react";
import { useState } from "react";
import Button from "../../atoms/Button";
import Checkbox from "../../atoms/Checkbox";
import Spinner from "../../atoms/Spinner";
import { Avatar } from "../../atoms/primitives";
import { Copy } from "../../atoms/icons";
import EmptyState from "../../components/EmptyState";
import Modal from "../../components/Modal";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import StWhatSOnAdminTabs from "./StWhatSOnAdminTabs";
import "../../admin/admin.css";

const poster = (hue: number, image?: string | null): CSSProperties =>
  image
    ? { backgroundImage: `url("${image}")` }
    : {
        backgroundImage: `linear-gradient(150deg, hsl(${hue} 70% 52%) 0%, hsl(${(hue + 40) % 360} 60% 28%) 100%)`,
      };

const REJECT_REASONS = [
  { code: "invalid_image", title: "Invalid image", description: "Does not comply with our Terms or Code of Ethics" },
  { code: "invalid_event_name", title: "Invalid hangout name", description: "Does not comply with our Terms or Code of Ethics" },
  { code: "inappropriate_description", title: "Inappropriate description", description: "Contains language that is not allowed" },
  { code: "invalid_duration", title: "Invalid duration", description: "Too short or longer than 24 hours" },
  { code: "invalid_location", title: "Invalid location", description: "Incorrect coordinates" },
];

export type EventItem = {
  id: string;
  name: string;
  creator: string;
  time: string;
  dateLabel: string;
  hue: number;
  image?: string | null;
};

type EventStatus = "pending" | "approved" | "rejected";

const STATUS: Record<EventStatus, { label: string; tone: "warn" | "ok" | "bad" }> = {
  pending: { label: "Pending", tone: "warn" },
  approved: { label: "Approved", tone: "ok" },
  rejected: { label: "Rejected", tone: "bad" },
};

const ClockIcon = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
    <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16Zm.5-13H11v6l5.25 3.15.75-1.23-4.5-2.67V7Z" />
  </svg>
);
const CalendarPlus = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
    <path d="M19 4h-1V2h-2v2H8V2H6v2H5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h7v-2H5V9h14v2h2V6a2 2 0 0 0-2-2Zm-1 14h-3v3h-2v-3h-3v-2h3v-3h2v3h3v2Z" />
  </svg>
);

function PendingEventCard({
  event,
  status,
  onOpen,
}: {
  event: EventItem;
  status: EventStatus;
  onOpen?: (event: EventItem, status: EventStatus) => void;
}) {
  return (
    <article className="adm-card" aria-disabled={status === "pending"}>
      <div className="adm-pills">
        {event.dateLabel ? <span className="adm-status">{event.dateLabel}</span> : null}
        <span className="adm-status" data-tone={STATUS[status].tone}>{STATUS[status].label}</span>
      </div>
      <div className="adm-thumb" style={poster(event.hue, event.image)} role="img" aria-label={event.name} />
      <h2 className="adm-card__title u-truncate">{event.name}</h2>
      <div className="adm-card__head" data-role="creator-row">
        <Avatar hue={event.hue} size={20} />
        <span className="adm-dim u-truncate">
          by <strong>{event.creator}</strong>
        </span>
      </div>
      <div data-role="time-pill">
        <span className="adm-status">
          <ClockIcon />
          {event.time}
        </span>
      </div>
      <div className="adm-actions adm-actions--split" data-role="hover-actions">
        <Button variant="ghost" size="sm">
          <CalendarPlus />
          Add to calendar
        </Button>
        <Button variant="ghost" size="sm" aria-label="Copy link">
          <Copy />
        </Button>
        <Button size="sm" aria-label={`Review ${event.name}`} onClick={() => onOpen && onOpen(event, status)}>
          Review
        </Button>
      </div>
    </article>
  );
}

function EventDetailModal({
  event,
  onClose,
  onApprove,
  onReject,
  processing,
}: {
  event: EventItem;
  onClose?: () => void;
  onApprove?: () => void;
  onReject?: () => void;
  processing?: boolean;
}) {
  return (
    <Modal onClose={onClose} width={560} ariaLabel={event.name}>
      <div className="adm__inner">
        <div className="adm-thumb" style={poster(event.hue, event.image)} role="img" aria-label={event.name} />
        <h2 className="adm__h2">{event.name}</h2>
        <div className="adm-card__head">
          <Avatar hue={event.hue} size={28} />
          <span className="adm-dim">
            by <strong>{event.creator}</strong>
          </span>
        </div>
        <div>
          <span className="adm-status">
            <ClockIcon />
            {event.dateLabel} &#xB7; {event.time}
          </span>
        </div>
        <p className="adm-card__text">
          A community-submitted hangout awaiting moderation. Review the poster, title, description and location
          before approving or rejecting.
        </p>
        <div className="adm-actions">
          <Button variant="secondary" tone="danger" disabled={processing} onClick={onReject}>
            Reject
          </Button>
          <Button tone="success" disabled={processing} onClick={onApprove}>
            Approve
          </Button>
        </div>
      </div>
    </Modal>
  );
}

function RejectEventModal({
  onClose,
  onSubmit,
  isSubmitting,
}: {
  onClose?: () => void;
  onSubmit?: (payload: { reasons: string[]; notes: string }) => void;
  isSubmitting?: boolean;
}) {
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [notes, setNotes] = useState("");
  const [showError, setShowError] = useState(false);

  function toggle(code: string) {
    setShowError(false);
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(code) ? next.delete(code) : next.add(code);
      return next;
    });
  }

  function submit() {
    if (selected.size === 0) {
      setShowError(true);
      return;
    }
    onSubmit && onSubmit({ reasons: Array.from(selected), notes: notes.trim() });
  }

  return (
    <Modal onClose={onClose} width={520} ariaLabel="Reject Reason">
      <div className="adm__inner">
        <h2 className="adm__h2">Reject Reason*</h2>
        <ul className="adm-list">
          {REJECT_REASONS.map((reason) => (
            <li key={reason.code}>
              <Checkbox checked={selected.has(reason.code)} onChange={() => toggle(reason.code)}>
                <strong>{reason.title}</strong> &#x2013; ({reason.description})
              </Checkbox>
            </li>
          ))}
        </ul>
        {showError && <p className="adm-bad">Select at least one reason</p>}
        <div className="adm-field">
          <label className="adm-field__label" htmlFor="reject-notes">Other (optional)</label>
          <textarea
            id="reject-notes"
            className="adm-input"
            rows={3}
            maxLength={2000}
            placeholder="User will receive a notification, be as descriptive as you can."
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
          />
        </div>
        <div className="adm-actions">
          <Button variant="secondary" disabled={isSubmitting} onClick={onClose}>
            Cancel
          </Button>
          <Button tone="danger" disabled={isSubmitting} onClick={submit}>
            Submit
          </Button>
        </div>
      </div>
    </Modal>
  );
}

function EventGrid({ events, status, empty, onOpen }: {
  events: EventItem[];
  status: EventStatus;
  empty: string;
  onOpen: (event: EventItem, status: EventStatus) => void;
}) {
  if (events.length === 0) return <p className="adm-card adm-card--dashed adm-dim">{empty}</p>;
  return (
    <div className="adm-grid">
      {events.map((event) => (
        <PendingEventCard key={event.id} event={event} status={status} onOpen={onOpen} />
      ))}
    </div>
  );
}

type StWhatSOnAdminPendingEventsProps = {
  chrome?: boolean;
  pending?: EventItem[];
  approved?: EventItem[];
  allowed?: boolean;
  loading?: boolean;
};

export default function StWhatSOnAdminPendingEvents({
  chrome = true,
  pending = [],
  approved = [],
  allowed = true,
  loading = false,
}: StWhatSOnAdminPendingEventsProps) {
  const [detail, setDetail] = useState<{ event: EventItem; status: EventStatus } | null>(null);
  const [rejecting, setRejecting] = useState(false);

  const openDetail = (event: EventItem, status: EventStatus) => setDetail({ event, status });
  const closeDetail = () => setDetail(null);
  const isPendingDetail = detail !== null && detail.status === "pending";

  return (
    <SitesChromeMaybe chrome={chrome} active="play">
      <div className="adm">
        <StWhatSOnAdminTabs active="pending" />

        <div className="adm__page">
          <div className="adm__inner">
            {loading ? (
              <div className="adm-gate" aria-busy="true">
                <div className="adm-card adm-card--dashed">
                  <Spinner size={32} />
                  <p className="adm-card__text">Loading pending hangouts&#x2026;</p>
                </div>
              </div>
            ) : !allowed ? (
              <div className="adm-gate" role="alert">
                <EmptyState
                  variant="inline"
                  title="Not authorized"
                  titleAs="h1"
                  subtitle="You don't have permission to review pending hangouts."
                />
              </div>
            ) : (
              <>
                <h1 className="adm__title">Pending Hangouts</h1>
                <EventGrid events={pending} status="pending" empty="No hangouts waiting for approval" onOpen={openDetail} />
                <h2 className="adm__h2">
                  Recently Approved <span className="adm-dim">(Last 24hs)</span>
                </h2>
                <EventGrid
                  events={approved}
                  status="approved"
                  empty="No hangouts approved in the last 24 hours"
                  onOpen={openDetail}
                />
              </>
            )}
          </div>
        </div>

        {detail && (
          <EventDetailModal
            event={detail.event}
            onClose={closeDetail}
            processing={false}
            onApprove={closeDetail}
            onReject={isPendingDetail ? () => setRejecting(true) : undefined}
          />
        )}

        {rejecting && (
          <RejectEventModal
            isSubmitting={false}
            onClose={() => setRejecting(false)}
            onSubmit={() => {
              setRejecting(false);
              closeDetail();
            }}
          />
        )}
      </div>
    </SitesChromeMaybe>
  );
}
