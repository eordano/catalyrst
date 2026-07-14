import type { CSSProperties } from "react";
import "./eventdetail.css";

type EventData = {
  title?: string;
  when?: string;
  host?: string;
  description?: string;
  schedule?: string;
  location?: string;
  image?: string;
  url?: string;
  start?: string | number | Date;
  finish?: string | number | Date;
  id?: string | number;
  jumpHref?: string;
};

const EVENT: EventData = {
  title: "PRIDE EDITION: Watch scary movies with Cult Horror Club",
  when: "FRI, JUN 19 @ 10:00PM",
  host: "Decentraland Foundation",
  description:
    "Watch classic cult horror movies together in Decentraland Theatre, from " +
    "haunted houses, zombies and vampires to strange old-school sci-fi and " +
    "psychological horror. Perfect for hanging out, reacting in chat, " +
    "discovering weird cinema, and sharing OG scares from the history of horror.\n\n" +
    "Three screenings every Friday — at 5am UTC, 2pm UTC & 8pm UTC.\n\n" +
    "What to see this season:\n\n" +
    "**June 19 - Night Tide, 1961** PRIDE SCREENING\n\n" +
    "A young sailor falls for Mora, a mysterious woman who performs as a mermaid " +
    "and may believe she is dangerous. This is our Pride screening because it " +
    "was written and directed by Curtis Harrington, a gay filmmaker recognised " +
    "as an important early figure in queer cinema.\n\n" +
    "**June 26 - The Brain That Wouldn't Die, 1962**",
  schedule: "Saturday, Jun 20 from 12:00am to 02:30am (UTC+3)",
  location: "(0,0)",
};

function icsStamp(d: Date): string {
  return d.toISOString().replace(/[-:]/g, "").replace(/\.\d{3}/, "");
}

function icsEscape(s?: string): string {
  return String(s || "")
    .replace(/\\/g, "\\\\")
    .replace(/\n/g, "\\n")
    .replace(/,/g, "\\,")
    .replace(/;/g, "\\;");
}

function downloadIcs(ev: EventData, { withAlarm }: { withAlarm?: boolean } = {}): boolean {
  if (typeof document === "undefined") return false;
  const start = ev.start ? new Date(ev.start) : null;
  if (!start || Number.isNaN(start.getTime())) return false;
  const end =
    ev.finish && !Number.isNaN(new Date(ev.finish).getTime())
      ? new Date(ev.finish)
      : new Date(start.getTime() + 60 * 60 * 1000);
  const lines = [
    "BEGIN:VCALENDAR",
    "VERSION:2.0",
    "PRODID:-//Decentraland//Events//EN",
    "BEGIN:VEVENT",
    `UID:${(ev.id || start.getTime())}@decentraland.org`,
    `DTSTAMP:${icsStamp(new Date())}`,
    `DTSTART:${icsStamp(start)}`,
    `DTEND:${icsStamp(end)}`,
    `SUMMARY:${icsEscape(ev.title)}`,
    `DESCRIPTION:${icsEscape(ev.description)}`,
    `LOCATION:${icsEscape(ev.url || ev.location)}`,
  ];
  if (withAlarm)
    lines.push(
      "BEGIN:VALARM",
      "TRIGGER:-PT15M",
      "ACTION:DISPLAY",
      "DESCRIPTION:Reminder",
      "END:VALARM",
    );
  lines.push("END:VEVENT", "END:VCALENDAR");
  const blob = new Blob([lines.join("\r\n")], { type: "text/calendar" });
  const href = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = href;
  a.download = `${(ev.title || "event").slice(0, 60).replace(/[^\w-]+/g, "_")}.ics`;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(href), 0);
  return true;
}

function shareEvent(ev: EventData) {
  if (typeof window === "undefined") return;
  const url = ev.url || ev.jumpHref || window.location.href;
  const data = { title: ev.title, text: ev.title, url };
  if (navigator.share) {
    navigator.share(data).catch(() => {});
    return;
  }
  try {
    navigator.clipboard?.writeText(url);
  } catch {
  }
}

type EventDetailProps = {
  event?: EventData;
  jumpHref?: string;
  onJumpIn?: () => void;
  onClose?: () => void;
};

export default function EventDetail({ event, jumpHref, onJumpIn, onClose }: EventDetailProps = {}) {
  const e = event || EVENT;
  const evForActions: EventData = { ...e, url: e.url || jumpHref };
  const remindOrCalendar = (withAlarm: boolean) => {
    if (!downloadIcs(evForActions, { withAlarm })) shareEvent(evForActions);
  };
  const heroStyle: (CSSProperties & { "--thumb-img": string }) | undefined =
    e.image ? { "--thumb-img": `url("${e.image}")` } : undefined;
  return (
    <div className="ep__backdrop evd__backdrop" onClick={onClose}>
      <div className="evd" onClick={(ev) => ev.stopPropagation()}>
        <div
          className="evd__hero"
          style={heroStyle}
          aria-hidden="true"
        >
          <div className="evd__heroscrim" />
          <button className="ep__close evd__close" aria-label="Close" onClick={onClose} data-sb-linkto="Explorer/Pages/Events">
            ×
          </button>
        </div>

        <div className="evd__meta">
          <div className="evd__when">{e.when}</div>
          <h2 className="evd__title">{e.title}</h2>
          <p className="evd__host">
            Hosted by <b>{e.host}</b>
          </p>
        </div>

        <div className="evd__body">
          <div className="evd__actions">
            <button
              className="evd__remind"
              type="button"
              onClick={() => remindOrCalendar(true)}
            >
              <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
                <path
                  d="M8 2a3.4 3.4 0 00-3.4 3.4c0 3.2-1.1 4.3-1.1 4.3h9c0 0-1.1-1.1-1.1-4.3A3.4 3.4 0 008 2zM6.6 11.7a1.4 1.4 0 002.8 0"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.3"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
              REMIND ME
            </button>
            <button
              className="evd__iconbtn"
              type="button"
              aria-label="Add to calendar"
              onClick={() => remindOrCalendar(false)}
            >
              <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
                <path
                  d="M3 4h10v9H3zM3 6.5h10M6 2.5v2M10 2.5v2"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.3"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </button>
            <button
              className="evd__iconbtn"
              type="button"
              aria-label="Share"
              onClick={() => shareEvent(evForActions)}
            >
              <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
                <g
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.3"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <circle cx="12" cy="3.5" r="1.7" />
                  <circle cx="4" cy="8" r="1.7" />
                  <circle cx="12" cy="12.5" r="1.7" />
                  <path d="M10.5 4.4 5.5 7.1M5.5 8.9l5 2.7" />
                </g>
              </svg>
            </button>
          </div>
        </div>

        <div
          className="evd__descblock"
          role="region"
          aria-label="Event description"
          tabIndex={0}
        >
          <h3 className="evd__sectitle">DESCRIPTION</h3>
          <p className="evd__desc">{e.description}</p>
        </div>

        <div className="evd__footer">
            <div className="evd__metarow">
              <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                <path
                  d="M3 4h10v9H3zM3 6.5h10M6 2.5v2M10 2.5v2"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.3"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
              <span>{e.schedule}</span>
            </div>
            <div className="evd__metarow evd__metarow--cta">
              <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
                <path
                  d="M8 1.5c2.5 0 4.5 2 4.5 4.5 0 3.2-4.5 8.5-4.5 8.5S3.5 9.2 3.5 6A4.5 4.5 0 018 1.5zM8 4.4a1.7 1.7 0 100 3.4 1.7 1.7 0 000-3.4z"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.3"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
              <span>{e.location}</span>
              {!onJumpIn && jumpHref ? (
                <a
                  className="evd__jumpin"
                  href={jumpHref}
                  target="_blank"
                  rel="noopener noreferrer"
                  data-sb-linkto="Explorer/Workflows/SceneLoading"
                >
                  JUMP IN
                  <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
                    <path
                      d="M3 8h9M8.5 4l4 4-4 4"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.6"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                </a>
              ) : (
                <button className="evd__jumpin" onClick={onJumpIn} data-sb-linkto="Explorer/Workflows/SceneLoading">
                  JUMP IN
                  <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
                    <path
                      d="M3 8h9M8.5 4l4 4-4 4"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.6"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                </button>
              )}
            </div>
        </div>
      </div>
    </div>
  );
}
