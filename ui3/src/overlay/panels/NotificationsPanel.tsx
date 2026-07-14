import { useState } from "react";
import type React from "react";

import "../../explorer/components/notifications.css";
import "./notificationspanel.css";

export const NOTIFICATION_CATEGORIES = [
  "friends",
  "badge",
  "gift",
  "community",
  "marketplace",
  "system",
] as const;
export type NotificationCategory = (typeof NOTIFICATION_CATEGORIES)[number];

const CATEGORY_LABEL: Record<NotificationCategory, string> = {
  friends: "Friends",
  badge: "Badge",
  gift: "Gift",
  community: "Community",
  marketplace: "Marketplace",
  system: "System",
};

export type Notification = {
  id: string;
  type: string;
  /** Epoch millis as a STRING -- the wire's i64-serialized-as-str form. */
  timestamp: string;
  read: boolean;
  metadata: Record<string, unknown>;
};

function categoryForType(type: string): NotificationCategory {
  const t = type.toLowerCase();
  if (t.startsWith("social_service_friendship")) return "friends";
  if (t === "friend_first_wear") return "friends";
  if (t === "badge_granted") return "badge";
  if (t === "tip_received" || t === "transfer_received") return "gift";
  if (t.startsWith("community_")) return "community";
  if (
    t === "item_sold" ||
    t === "item_published" ||
    t === "bid_accepted" ||
    t === "bid_received" ||
    t === "royalties_earned" ||
    t.startsWith("credits_") ||
    t.startsWith("rental_")
  ) {
    return "marketplace";
  }
  return "system";
}

function humanizeType(type: string): string {
  return type
    .split("_")
    .map((w) => (w ? w.charAt(0).toUpperCase() + w.slice(1) : w))
    .join(" ");
}

function notificationTitle(n: Notification): string {
  const t = n.metadata?.["title"];
  if (typeof t === "string" && t.trim()) return t;
  return humanizeType(n.type);
}

function notificationBody(n: Notification): string {
  const d = n.metadata?.["description"];
  return typeof d === "string" ? d : "";
}

export function notificationLink(n: Notification): string | null {
  const l = n.metadata?.["link"];
  return typeof l === "string" && /^https?:\/\//.test(l) ? l : null;
}

function notificationImage(n: Notification): string | null {
  const m = n.metadata ?? {};
  for (const k of ["badgeImageUrl", "thumbnailUrl", "tokenImage", "imageUrl", "image"]) {
    const v = m[k];
    if (typeof v === "string" && v.trim()) return v;
  }
  return null;
}

function unreadCount(rows: Notification[]): number {
  return rows.filter((n) => !n.read).length;
}

function filterByCategory(rows: Notification[], category: string): Notification[] {
  if (!category || category === "all") return rows;
  return rows.filter((n) => categoryForType(n.type) === category);
}

function markRead(
  rows: Notification[],
  ids: "all" | readonly string[],
): Notification[] {
  const all = ids === "all";
  const set = all ? null : new Set(ids);
  return rows.map((n) =>
    n.read || (set ? !set.has(n.id) : !all) ? n : { ...n, read: true },
  );
}

function relativeTime(timestamp: number, now: number): string {
  const diff = Math.max(0, now - timestamp);
  const min = Math.floor(diff / 60000);
  if (min < 1) return "Just Now";
  if (min < 60) return `${min}m`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h`;
  const day = Math.floor(hr / 24);
  if (day === 1) return "Yesterday";
  return `${day}d`;
}

const svg = (children: React.ReactNode) => (
  <svg
    viewBox="0 0 20 20"
    width="18"
    height="18"
    aria-hidden="true"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.7"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    {children}
  </svg>
);
const ICON: Record<NotificationCategory, React.ReactNode> = {
  friends: svg(
    <>
      <circle cx="10" cy="6.5" r="3" />
      <path d="M4 16c0-3 2.7-4.5 6-4.5s6 1.5 6 4.5" />
    </>,
  ),
  badge: svg(
    <>
      <circle cx="10" cy="8" r="4.5" />
      <path d="M7 12l-1.5 5 4.5-2.5 4.5 2.5L13 12" />
    </>,
  ),
  gift: svg(
    <>
      <rect x="3.5" y="8" width="13" height="9" rx="1" />
      <path d="M3 8h14M10 8v9M10 8s-1-4-3.5-4S6 8 10 8s1-4 3.5-4S14 8 10 8" />
    </>,
  ),
  community: svg(
    <>
      <circle cx="10" cy="11" r="2" />
      <path d="M6 7a6 6 0 0 0 0 8M14 7a6 6 0 0 1 0 8M4 5a9 9 0 0 0 0 12M16 5a9 9 0 0 1 0 12" />
    </>,
  ),
  marketplace: svg(
    <>
      <rect x="3" y="5.5" width="14" height="9" rx="1.5" />
      <path d="M3 8.5h14M6 12h3" />
    </>,
  ),
  system: svg(
    <>
      <path d="M10 3l6 3.5v7L10 17l-6-3.5v-7z" />
      <path d="M4 6.5l6 3.5 6-3.5M10 10v7" />
    </>,
  ),
};

const TINT: Record<NotificationCategory, string> = {
  friends: "linear-gradient(135deg,#ff8f5e,#c44dff)",
  badge: "linear-gradient(135deg,#ffb347,#ff7a00)",
  gift: "linear-gradient(135deg,#5ee0ff,#3a6dff)",
  community: "linear-gradient(135deg,#6a2da8,#2d8cff)",
  marketplace: "linear-gradient(135deg,#7a16a8,#c44dff)",
  system: "linear-gradient(135deg,#3a6dff,#6a2da8)",
};

export type NotificationsPanelProps = {
  notifications: Notification[];
  filter: string;
  now: number;
  showFilters: boolean;
  showMarkAll: boolean;
  /**
   * Set when the feed could not be read. An unread feed and an empty one are
   * different answers and must not share the "No notifications yet." line.
   */
  unavailable: string | null;
  onFilter: (category: string) => void;
  onMarkRead: (n: Notification) => void;
  onMarkAll: (count: number) => void;
};

export default function NotificationsPanel({
  notifications,
  filter,
  now,
  showFilters,
  showMarkAll,
  unavailable,
  onFilter,
  onMarkRead,
  onMarkAll,
}: NotificationsPanelProps) {
  const [rows, setRows] = useState<Notification[]>(notifications);

  const visible = filterByCategory(rows, filter);
  const unread = unreadCount(rows);

  function handleCard(n: Notification) {
    if (!n.read) {
      setRows((prev) => markRead(prev, [n.id]));
      onMarkRead(n);
    }
    const link = notificationLink(n);
    if (link) window.open(link, "_blank", "noopener,noreferrer");
  }

  function handleMarkAll() {
    const toClear = unread;
    if (toClear === 0) return;
    setRows((prev) => markRead(prev, "all"));
    onMarkAll(toClear);
  }

  return (
    <div className="nf">
      <div className="nfx__header">
        <span className="nfx__titlewrap">
          <span className="nf__title">NOTIFICATIONS</span>
          {unread > 0 && <span className="nfx__unread">{unread}</span>}
        </span>
        {showMarkAll && (
          <button
            type="button"
            className="nfx__markall"
            onClick={handleMarkAll}
            disabled={unread === 0}
          >
            Mark all read
          </button>
        )}
      </div>

      {showFilters && (
        <div className="nfx__filters" role="tablist" aria-label="Notification categories">
          <FilterPill
            label="All"
            active={filter === ""}
            onSelect={() => onFilter("")}
          />
          {NOTIFICATION_CATEGORIES.map((c) => (
            <FilterPill
              key={c}
              label={CATEGORY_LABEL[c]}
              active={filter === c}
              onSelect={() => onFilter(c)}
            />
          ))}
        </div>
      )}

      <div className="nf__list">
        {visible.length === 0 ? (
          unavailable ? (
            <p className="nfx__failed" role="alert">
              We couldn't load your notifications: {unavailable}. This is not an
              empty inbox {"\u{2014}"} nothing was read.
            </p>
          ) : (
            <p className="nfx__empty">
              {rows.length === 0
                ? "No notifications yet."
                : "No notifications in this category."}
            </p>
          )
        ) : (
          visible.map((n) => {
            const cat = categoryForType(n.type);
            const img = notificationImage(n);
            return (
              <button
                type="button"
                key={n.id}
                className={
                  "nf__card nf__card--" + cat + " nfx__card" + (n.read ? " is-read" : "")
                }
                onClick={() => handleCard(n)}
                aria-label={
                  notificationTitle(n) +
                  (notificationLink(n) ? " (opens link)" : "") +
                  (n.read ? " (read)" : " (unread, mark read)")
                }
              >
                {!n.read && <span className="nfx__dot" aria-hidden="true" />}
                {img ? (
                  <span
                    className="nf__icon"
                    style={{ backgroundImage: `url(${img})`, backgroundSize: "cover", backgroundPosition: "center" }}
                    aria-hidden="true"
                  />
                ) : (
                <span
                  className="nf__icon"
                  style={{ background: TINT[cat] }}
                >
                  {ICON[cat]}
                </span>
                )}
                <div className="nf__content">
                  <span className="nf__cardtitle">{notificationTitle(n)}</span>
                  <div className="nf__body">{notificationBody(n)}</div>
                  <span className="nf__time">{relativeTime(Number(n.timestamp), now)}</span>
                </div>
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}

type FilterPillProps = {
  label: string;
  active: boolean;
  onSelect: () => void;
};

function FilterPill({ label, active, onSelect }: FilterPillProps) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      className={"nfx__pill" + (active ? " is-active" : "")}
      onClick={onSelect}
    >
      {label}
    </button>
  );
}

export type NotificationsStageProps = NotificationsPanelProps & {
  open: boolean;
  onBell: () => void;
};

export function NotificationsStage({
  open,
  onBell,
  notifications,
  filter,
  now,
  showFilters,
  showMarkAll,
  unavailable,
  onFilter,
  onMarkRead,
  onMarkAll,
}: NotificationsStageProps) {
  return (
    <div className="nf__stage">
      <nav className="nf__rail" aria-label="HUD sidebar">
        <span className="nf__rail-avatar" aria-hidden="true" />
        <div className="nf__rail-group">
          <button
            type="button"
            className={"nf__rail-btn" + (open ? " is-active" : "")}
            aria-label="Notifications"
            title="Notifications"
            aria-pressed={open}
            onClick={onBell}
          >
            <Bell />
            {(() => {
              const unread = notifications.filter((n) => !n.read).length;
              return unread > 0 ? (
                <span className="nf__rail-badge">{unread}</span>
              ) : null;
            })()}
          </button>
        </div>
      </nav>

      {open ? (
        <NotificationsPanel
          notifications={notifications}
          filter={filter}
          now={now}
          showFilters={showFilters}
          showMarkAll={showMarkAll}
          unavailable={unavailable}
          onFilter={onFilter}
          onMarkRead={onMarkRead}
          onMarkAll={onMarkAll}
        />
      ) : (
        <p className="nfx__rest">
          Notifications closed.{" "}
          <a href="?panel=notifications">Open the panel</a> from the bell.
        </p>
      )}

      <noscript>
        <p className="nfx__rest">
          Enable JavaScript to filter notifications and mark them read.
        </p>
      </noscript>
    </div>
  );
}

function Bell() {
  return (
    <svg
      viewBox="0 0 20 20"
      width="16"
      height="16"
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M5.5 8a4.5 4.5 0 0 1 9 0c0 4 1.5 5 1.5 5H4s1.5-1 1.5-5z" />
      <path d="M8.5 16a1.5 1.5 0 0 0 3 0" />
    </svg>
  );
}
