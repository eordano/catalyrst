import type { CSSProperties, ReactNode } from "react";
import { useMemo, useState } from "react";
import { useHideMinimapWhileMounted } from "../../overlay/minimapVisibility";
import { requestFriendAction, FRIEND_ACTIONS } from "../../data/hooks/friendActions";
import "./notifications.css";

function Name({ name, tag }: { name: string; tag: string }) {
  return <><span className="nf__name">{name}</span><span className="nf__tag">{tag}</span></>;
}

const I = (d: ReactNode) => (
  <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"
    fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">{d}</svg>
);

type NotifType = "friends" | "badge" | "gift" | "community" | "marketplace" | "system";

const ICON: Record<NotifType, ReactNode> = {
  friends: I(<><circle cx="10" cy="6.5" r="3" /><path d="M4 16c0-3 2.7-4.5 6-4.5s6 1.5 6 4.5" /></>),
  badge: I(<><circle cx="10" cy="8" r="4.5" /><path d="M7 12l-1.5 5 4.5-2.5 4.5 2.5L13 12" /></>),
  gift: I(<><rect x="3.5" y="8" width="13" height="9" rx="1" /><path d="M3 8h14M10 8v9M10 8s-1-4-3.5-4S6 8 10 8s1-4 3.5-4S14 8 10 8" /></>),
  community: I(<><circle cx="10" cy="11" r="2" /><path d="M6 7a6 6 0 0 0 0 8M14 7a6 6 0 0 1 0 8M4 5a9 9 0 0 0 0 12M16 5a9 9 0 0 1 0 12" /></>),
  marketplace: I(<><rect x="3" y="5.5" width="14" height="9" rx="1.5" /><path d="M3 8.5h14M6 12h3" /></>),
  system: I(<><path d="M10 3l6 3.5v7L10 17l-6-3.5v-7z" /><path d="M4 6.5l6 3.5 6-3.5M10 10v7" /></>),
};
const CLOSE = (
  <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true"
    fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M6 6l8 8M14 6l-8 8" />
  </svg>
);

const R = (d: ReactNode) => (
  <svg viewBox="0 0 20 20" width="16" height="16" aria-hidden="true"
    fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">{d}</svg>
);

type RailItem = { key: string; active?: boolean; icon: ReactNode; badge?: ReactNode };

const RAIL_TOP: RailItem[] = [
  { key: "bell", active: true, icon: R(<><path d="M5.5 8a4.5 4.5 0 0 1 9 0c0 4 1.5 5 1.5 5H4s1.5-1 1.5-5z" /><path d="M8.5 16a1.5 1.5 0 0 0 3 0" /></>) },
  { key: "badge", icon: R(<><circle cx="10" cy="8" r="4" /><path d="M7.5 11.5L6 17l4-2.2L14 17l-1.5-5.5" /></>) },
  { key: "backpack", icon: R(<><path d="M5 8a5 5 0 0 1 10 0v8a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1z" /><path d="M8 8a2 2 0 0 1 4 0M7.5 12h5" /></>) },
  { key: "map", icon: R(<><path d="M10 17s5-4.5 5-8a5 5 0 0 0-10 0c0 3.5 5 8 5 8z" /><circle cx="10" cy="9" r="1.8" /></>) },
  { key: "friends", icon: R(<><circle cx="8" cy="7" r="2.6" /><path d="M3.5 16c0-2.5 2-3.8 4.5-3.8s4.5 1.3 4.5 3.8" /><path d="M13 5.5a2.4 2.4 0 0 1 0 4.7M14.5 16c0-2-.8-3.1-2.2-3.6" /></>) },
];
const RAIL_BOTTOM: RailItem[] = [
  { key: "settings", icon: R(<><circle cx="10" cy="10" r="2.5" /><path d="M10 3v2M10 15v2M3 10h2M15 10h2M5 5l1.4 1.4M13.6 13.6L15 15M15 5l-1.4 1.4M6.4 13.6L5 15" /></>) },
  { key: "help", icon: R(<><circle cx="10" cy="10" r="7" /><path d="M8 8a2 2 0 1 1 3 1.7c-.7.5-1 .9-1 1.8" /><circle cx="10" cy="14" r=".5" fill="currentColor" stroke="none" /></>) },
];

type NotifAction = { label: string; primary?: boolean; to?: string; href?: string };
type NotifItem = {
  id: string;
  type: NotifType;
  title: string;
  time: string;
  read?: boolean;
  avatar?: string;
  thumb?: string;
  iconBg?: string;
  friendAddress?: string;
  body: ReactNode;
  actions?: NotifAction[];
};

const ITEMS: NotifItem[] = [
  {
    id: "friend-morat", type: "friends", title: "Friend Request Received", time: "Just Now", read: false,
    avatar: "linear-gradient(135deg,#ff8f5e,#c44dff)",
    body: <><Name name="Morat" tag="#1r56" /> wants to be your friend.</>,
    actions: [{ label: "Accept", primary: true }, { label: "Decline" }],
  },
  { id: "badge-weekly", type: "badge", title: "New Badge Unlocked!", time: "2m", read: false,
    iconBg: "linear-gradient(135deg,#ffb347,#ff7a00)",
    body: "Weekly Goal Completed!",
    actions: [{ label: "View Badge", primary: true, to: "Explorer/Pages/BadgesDetails" }] },
  { id: "gift-player", type: "gift", title: "Gift Received", time: "10m", read: false,
    avatar: "linear-gradient(135deg,#5ee0ff,#3a6dff)",
    body: <><Name name="PlayerName" tag="#4fasd" /> sent you a Gift!</>,
    actions: [{ label: "Open Gift", primary: true }] },
  { id: "community-dao", type: "community", title: "Community Voice Stream Started", time: "1h", read: false,
    thumb: "linear-gradient(135deg,#6a2da8,#2d8cff)",
    body: "The Decentraland DAO is streaming! Click here to join.",
    actions: [{ label: "Join Stream", primary: true, to: "Explorer/Components/CommunityStream" }] },
  { id: "market-credits", type: "marketplace", title: "Marketplace Credits", time: "3h", read: true,
    iconBg: "linear-gradient(135deg,#7a16a8,#c44dff)",
    body: "Claim your Credits to unlock them.",
    actions: [{ label: "Claim", primary: true }] },
  { id: "system-item", type: "system", title: "New Item Received", time: "Yesterday", read: true,
    iconBg: "linear-gradient(135deg,#3a6dff,#6a2da8)",
    body: "Long hair is already in your Backpack.",
    actions: [{ label: "View in Backpack", to: "Explorer/Pages/Backpack" }] },
];

type PanelProps = {
  items?: NotifItem[];
  onMarkRead?: (id: string) => void;
  onMarkAllRead?: () => void;
};

function Panel({ items: itemsProp, onMarkRead, onMarkAllRead }: PanelProps) {
  const source = itemsProp ?? ITEMS;
  const [dismissed, setDismissed] = useState<ReadonlySet<string>>(() => new Set());

  const items = useMemo(
    () => source.filter((it) => !dismissed.has(it.id)),
    [source, dismissed],
  );
  const unread = items.filter((it) => it.read === false).length;

  const dismiss = (id: string) =>
    setDismissed((cur) => new Set(cur).add(id));
  const clearAll = () =>
    setDismissed((cur) => {
      const next = new Set(cur);
      for (const it of source) next.add(it.id);
      return next;
    });

  return (
    <div className="nf">
      <div className="nf__header">
        <span className="nf__titlewrap">
          <span className="nf__title">NOTIFICATIONS</span>
          {unread > 0 && <span className="nf__unread-badge">{unread > 99 ? "99+" : unread}</span>}
        </span>
        <div className="nf__headeractions">
          {unread > 0 && onMarkAllRead && (
            <button type="button" className="nf__markread" onClick={onMarkAllRead}>Mark all read</button>
          )}
          {items.length > 0 && (
            <button type="button" className="nf__clear" onClick={clearAll}>Clear all</button>
          )}
        </div>
      </div>
      {items.length === 0 ? (
        <div className="nf__empty">
          <span className="nf__empty-icon" aria-hidden="true">
            {R(<><path d="M5.5 8a4.5 4.5 0 0 1 9 0c0 4 1.5 5 1.5 5H4s1.5-1 1.5-5z" /><path d="M8.5 16a1.5 1.5 0 0 0 3 0" /></>)}
          </span>
          <span className="nf__empty-title">You're all caught up</span>
          <span className="nf__empty-sub">New notifications will show up here.</span>
        </div>
      ) : (
        <div className="nf__list">
          {items.map((it) => {
            const iconStyle: CSSProperties | undefined = it.iconBg ? { background: it.iconBg } : undefined;
            const isUnread = it.read === false;
            return (
              <div
                className={"nf__card nf__card--" + it.type + (isUnread ? " nf__card--unread" : "")}
                key={it.id}
                onClick={isUnread ? () => onMarkRead?.(it.id) : undefined}
              >
                {isUnread && <span className="nf__unread-dot" aria-label="unread" />}
                {it.avatar ? (
                  <span className="nf__avatar" aria-hidden="true" style={{ background: it.avatar }} />
                ) : it.thumb ? (
                  <span className="nf__thumb" aria-hidden="true" style={{ background: it.thumb }} />
                ) : (
                  <span className="nf__icon" style={iconStyle}>{ICON[it.type]}</span>
                )}
                <div className="nf__content">
                  <span className="nf__cardtitle">{it.title}</span>
                  <div className="nf__body">{it.body}</div>
                  <span className="nf__time">{it.time}</span>
                  {it.actions && it.actions.length > 0 && (
                    <div className="nf__actions">
                      {it.actions.map((a) => {
                        const isFriendAct =
                          it.type === "friends" && (a.label === "Accept" || a.label === "Decline");
                        const inline = isFriendAct && !!it.friendAddress;
                        if (a.href) {
                          return (
                            <a
                              key={a.label}
                              className={"nf__action" + (a.primary ? " nf__action--primary" : "")}
                              href={a.href}
                              target="_blank"
                              rel="noopener noreferrer"
                            >
                              {a.label}
                            </a>
                          );
                        }
                        return (
                          <button
                            key={a.label}
                            type="button"
                            className={"nf__action" + (a.primary ? " nf__action--primary" : "")}
                            data-sb-linkto={inline ? undefined : a.to || undefined}
                            onClick={() => {
                              if (inline && it.friendAddress) {
                                requestFriendAction(
                                  a.label === "Accept" ? FRIEND_ACTIONS.ACCEPT : FRIEND_ACTIONS.REJECT,
                                  it.friendAddress,
                                );
                              }
                              dismiss(it.id);
                            }}
                          >
                            {a.label}
                          </button>
                        );
                      })}
                    </div>
                  )}
                </div>
                <button
                  type="button"
                  className="nf__dismiss"
                  aria-label="Dismiss notification"
                  title="Dismiss"
                  onClick={() => dismiss(it.id)}
                >
                  {CLOSE}
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

type NotificationsProps = {
  bare?: boolean;
  items?: NotifItem[];
  floating?: boolean;
  onMarkRead?: (id: string) => void;
  onMarkAllRead?: () => void;
};

export default function Notifications({
  bare = false,
  items,
  floating = false,
  onMarkRead,
  onMarkAllRead,
}: NotificationsProps) {
  useHideMinimapWhileMounted(floating);

  if (bare) {
    return (
      <div className={"nf__bare" + (floating ? " nf__bare--floating" : "")}>
        <Panel items={items} onMarkRead={onMarkRead} onMarkAllRead={onMarkAllRead} />
      </div>
    );
  }
  const bellUnread = (items ?? ITEMS).filter((it) => it.read === false).length;
  return (
    <div className="nf__stage">
      <nav className="nf__rail" aria-label="HUD sidebar">
        <span className="nf__rail-avatar" aria-hidden="true" />
        <div className="nf__rail-group">
          {RAIL_TOP.map((b) => (
            <button key={b.key} className={"nf__rail-btn" + (b.active ? " is-active" : "")}
              aria-label={b.key} title={b.key}>
              {b.icon}
              {b.key === "bell" && bellUnread > 0 ? (
                <span className="nf__rail-badge">{bellUnread > 99 ? "99+" : bellUnread}</span>
              ) : b.badge ? (
                <span className="nf__rail-badge">{b.badge}</span>
              ) : null}
            </button>
          ))}
        </div>
        <div className="nf__rail-group nf__rail-group--bottom">
          {RAIL_BOTTOM.map((b) => (
            <button key={b.key} className="nf__rail-btn" aria-label={b.key} title={b.key}>{b.icon}</button>
          ))}
        </div>
      </nav>
      <Panel items={items} onMarkRead={onMarkRead} onMarkAllRead={onMarkAllRead} />
    </div>
  );
}
