import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { Avatar, Badge } from "../../atoms/primitives";
import Tooltip from "../../atoms/Tooltip";
import ContextMenu, { type ContextMenuItem } from "../../components/ContextMenu";
import { useBridgeState } from "../../overlay/bridge";
import { useMinimapVisibility } from "../../overlay/minimapVisibility";
import { SIDEBAR_LOWER, SIDEBAR_UPPER, type IconName, type NavItem } from "./sidebarNav";
import "./sidebar.css";

type IcoProps = { d?: string; viewBox?: string; sw?: number; children?: ReactNode };

function Ico({ d, viewBox = "0 0 24 24", sw, children }: IcoProps) {
  const body = d ? <path d={d} /> : children;
  return (
    <svg className="sb__icon" viewBox={viewBox} aria-hidden="true" focusable="false">
      {sw ? <g strokeWidth={sw}>{body}</g> : body}
    </svg>
  );
}

const ICONS: Record<IconName, ReactNode> = {
  overflow: <Ico><circle cx="5" cy="12" r="1.9" /><circle cx="12" cy="12" r="1.9" /><circle cx="19" cy="12" r="1.9" /></Ico>,
  bell: <Ico d="M12 2a6 6 0 0 0-6 6c0 3.6-1 5.4-1.8 6.3-.5.6-.1 1.7.8 1.7h14c.9 0 1.3-1.1.8-1.7C19 13.4 18 11.6 18 8a6 6 0 0 0-6-6Zm0 20a2.6 2.6 0 0 0 2.6-2.4H9.4A2.6 2.6 0 0 0 12 22Z" />,
  backpackRotate: (
    <Ico>
      <path d="M9 4.2A3 3 0 0 1 15 4.2V5h1.2a2.6 2.6 0 0 1 2.6 2.6V17a2.6 2.6 0 0 1-2.6 2.6H7.8A2.6 2.6 0 0 1 5.2 17V7.6A2.6 2.6 0 0 1 7.8 5H9v-.8Zm2 .8h2a1.5 1.5 0 0 0-2 0Zm-2.4 4.4v2.4h6.8V9.4H8.6Z" />
      <path d="M14.6 14.2a2.4 2.4 0 0 1-4.5.6l1.1.5a1.3 1.3 0 0 0 2.3-.3l-.9.1.7-1.4 1.4.8-.1.1Zm-5.2-.6a2.4 2.4 0 0 1 4.5-.6l-1.1-.5a1.3 1.3 0 0 0-2.3.3l.9-.1-.7 1.4-1.4-.8.1-.1Z" />
    </Ico>
  ),
  events: (
    <Ico sw={2}>
      <rect x="3.5" y="5" width="17" height="15.5" rx="4" />
      <path d="M3.5 10.5h17M8.3 2.8v4M15.7 2.8v4" />
    </Ico>
  ),
  places: <Ico d="M12 2a7 7 0 0 0-7 7c0 5 7 13 7 13s7-8 7-13a7 7 0 0 0-7-7Zm0 9.5A2.5 2.5 0 1 1 12 6.5a2.5 2.5 0 0 1 0 5Z" />,
  people: (
    <Ico sw={2}>
      <circle cx="12" cy="8.8" r="3.1" />
      <path d="M6.6 18.3a5.4 3.9 0 0 1 10.8 0Z" />
      <circle cx="5.1" cy="11.1" r="1.8" />
      <circle cx="18.9" cy="11.1" r="1.8" />
      <path d="M1.7 18.5c0-2.6 1.6-4.4 4-4.4 1.2 0 2.2.5 3 1.3" />
      <path d="M22.3 18.5c0-2.6-1.6-4.4-4-4.4-1.2 0-2.2.5-3 1.3" />
    </Ico>
  ),
  backpack: (
    <Ico>
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M8.2 5.4a3.8 3.8 0 0 1 7.6 0v.3A3.2 3.2 0 0 1 18.4 8.8V18a2.6 2.6 0 0 1-2.6 2.6H8.2A2.6 2.6 0 0 1 5.6 18V8.8A3.2 3.2 0 0 1 8.2 5.7v-.3Zm5.8.2V5.4a2 2 0 0 0-4 0v.2a3.2 3.2 0 0 1 .8-.1h2.4c.27 0 .54.04.8.1ZM8.8 8.4a1.4 1.4 0 0 0-1.4 1.4v8.2a.8.8 0 0 0 .8.8h.6v-3.4a1.4 1.4 0 0 1 1.4-1.4h3.6a1.4 1.4 0 0 1 1.4 1.4v3.4h.6a.8.8 0 0 0 .8-.8V9.8a1.4 1.4 0 0 0-1.4-1.4H8.8Zm5 8.2H10.2v2.2h3.6v-2.2Z"
      />
    </Ico>
  ),
  marketplace: <Ico d="M7 7V6a5 5 0 0 1 10 0v1h2.2l.8 12.5a2 2 0 0 1-2 2.1H6a2 2 0 0 1-2-2.1L4.8 7H7Zm2 0h6V6a3 3 0 0 0-6 0v1Z" />,
  gallery: <Ico d="M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Zm0 2v9.6l3.8-3.8 2.6 2.6 3.4-3.4L19 16.8V6H5Zm3.5 1.8a1.7 1.7 0 1 1 0 3.4 1.7 1.7 0 0 1 0-3.4Z" />,
  settings: <Ico d="M19.14 12.94a7.49 7.49 0 0 0 .05-.94 7.49 7.49 0 0 0-.05-.94l2.03-1.58a.5.5 0 0 0 .12-.62l-1.92-3.32a.5.5 0 0 0-.6-.22l-2.39.96a7 7 0 0 0-1.62-.94l-.36-2.54a.49.49 0 0 0-.49-.42h-3.84a.49.49 0 0 0-.49.42l-.36 2.54a7 7 0 0 0-1.62.94l-2.39-.96a.5.5 0 0 0-.6.22L2.74 8.86a.5.5 0 0 0 .12.62l2.03 1.58a7.49 7.49 0 0 0 0 1.88l-2.03 1.58a.5.5 0 0 0-.12.62l1.92 3.32a.5.5 0 0 0 .6.22l2.39-.96a7 7 0 0 0 1.62.94l.36 2.54a.49.49 0 0 0 .49.42h3.84a.49.49 0 0 0 .49-.42l.36-2.54a7 7 0 0 0 1.62-.94l2.39.96a.5.5 0 0 0 .6-.22l1.92-3.32a.5.5 0 0 0-.12-.62l-2.03-1.58zM12 15.6A3.6 3.6 0 1 1 12 8.4a3.6 3.6 0 0 1 0 7.2z" />,
  help: <Ico d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm.1 14.8a1.2 1.2 0 1 1 0 2.4 1.2 1.2 0 0 1 0-2.4Zm1.7-5.1c-.7.6-1 1-1 1.7v.4h-1.8v-.5c0-1.1.5-1.9 1.4-2.6.7-.5 1-.9 1-1.5 0-.7-.5-1.2-1.4-1.2-.8 0-1.4.5-1.6 1.3l-1.7-.4c.3-1.5 1.5-2.5 3.3-2.5 2 0 3.3 1.1 3.3 2.8 0 1-.5 1.7-1.5 2.5Z" />,
  voice: (
    <Ico>
      <path d="M9.4 6.5a4.6 4.6 0 1 0 .2 8.8" />
      <circle cx="10.2" cy="9.6" r="0.6" />
      <path d="M8.2 11.4c.5.7 1.4 1 2.1.5" />
      <path d="M14.2 8.6a3 3 0 0 1 0 5.2" />
      <path d="M15.8 7a5 5 0 0 1 0 8.4" />
    </Ico>
  ),
  wearables: <Ico d="M13 2 4 14h6l-1 8 9-12h-6l1-8Z" />,
  skybox: (
    <Ico>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 8a4 4 0 0 0 0 8Z" fill="currentColor" stroke="none" />
      <path d="M12 3v2.2M12 18.8V21M3 12h2.2M18.8 12H21M5.6 5.6l1.6 1.6M16.8 16.8l1.6 1.6M5.6 18.4l1.6-1.6M16.8 7.2l1.6-1.6" />
    </Ico>
  ),
  camera: <Ico d="M9 4 7.5 6H5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-2.5L15 4H9Zm3 5a4.2 4.2 0 1 1 0 8.4A4.2 4.2 0 0 1 12 9Zm0 2a2.2 2.2 0 1 0 0 4.4A2.2 2.2 0 0 0 12 11Z" />,
  emote: (
    <Ico>
      <circle cx="12" cy="4.2" r="1.9" />
      <path d="M3.2 5.4 5 2.9l5.4 5.7h3.2L19 2.9l1.8 2.5-4.4 5.2-.8 4.8 1.8 6-2.8.6-2.6-4.4-2.6 4.4-2.8-.6 1.8-6-.8-4.8Z" />
    </Ico>
  ),
  friends: <Ico d="M9 11.5a3.2 3.2 0 1 1 0-6.4 3.2 3.2 0 0 1 0 6.4Zm0 1.4c-3 0-5.5 1.6-5.5 3.8V19H11v-2.3c0-1.4.7-2.6 1.8-3.4a9 9 0 0 0-3.8-.4Zm8.1-3.9c1 .9 2.5 2.3 2.5 3.6 0 1.1-1 2-2.5 3.4-1.5-1.4-2.5-2.3-2.5-3.4 0-1.3 1.5-2.7 2.5-3.6Z" />,
  chat: <Ico d="M4 5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H9l-4 3.5V16H6a2 2 0 0 1-2-2V5Z" />,
};

const STROKE_ICONS = new Set<IconName>(["skybox", "voice", "people", "emote", "events"]);

export function SidebarGlyph({ name }: { name: IconName }) {
  return (
    <span className={"sb__glyph" + (STROKE_ICONS.has(name) ? " is-stroke" : "")}>{ICONS[name]}</span>
  );
}

type BtnProps = {
  icon: IconName;
  label: string;
  shortcut?: string;
  active?: boolean;
  tile?: boolean;
  badge?: ReactNode;
  badgeKind?: string;
  dot?: boolean;
  notifDot?: boolean;
  to?: string;
  panel?: string;
  menu?: boolean;
  expanded?: boolean;
  onClick?: () => void;
};

function Btn({ icon, label, shortcut, active, tile, badge, badgeKind, dot, notifDot, to, panel, menu, expanded, onClick }: BtnProps) {
  return (
    <Tooltip label={label} shortcut={shortcut} side="right">
      <button
        className={
          "sb__btn" +
          (active ? " is-active" : "") +
          (tile ? " is-tile" : "") +
          (STROKE_ICONS.has(icon) ? " is-stroke" : "")
        }
        aria-label={label}
        aria-haspopup={menu ? "menu" : undefined}
        aria-expanded={menu ? !!expanded : undefined}
        data-sb-linkto={to || undefined}
        data-sb-panel={panel}
        onClick={onClick}
      >
        {ICONS[icon]}
        {dot ? <span className="sb__presence" /> : null}
        {notifDot ? <span className="sb__notif" /> : null}
        {badge != null ? (
          <span className="sb__badge">
            <Badge tone={badgeKind === "purple" ? "purple" : "ruby"}>{badge}</Badge>
          </span>
        ) : null}
      </button>
    </Tooltip>
  );
}

const MENU_ICONS = {
  map: (
    <svg className="sb__menuicon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M9 4 3.5 6.2v13.3L9 17.3l6 2.4 5.5-2.2V4.2L15 6.4 9 4Zm0 0v13.3M15 6.4v13.3" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
    </svg>
  ),
  places: (
    <svg className="sb__menuicon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M12 2a7 7 0 0 0-7 7c0 5 7 13 7 13s7-8 7-13a7 7 0 0 0-7-7Zm0 9.5A2.5 2.5 0 1 1 12 6.5a2.5 2.5 0 0 1 0 5Z" />
    </svg>
  ),
};

function PlacesBtn({ item }: { item: NavItem }) {
  const { userHidden, toggleUserHidden } = useMinimapVisibility();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e: PointerEvent) => {
      if (ref.current && e.target instanceof Node && !ref.current.contains(e.target)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [open]);
  if (!userHidden) return <Btn {...item} />;
  const items: ContextMenuItem[] = [
    {
      kind: "button",
      label: "Show minimap",
      icon: MENU_ICONS.map,
      onClick: () => {
        setOpen(false);
        toggleUserHidden();
      },
    },
    { kind: "button", label: "Open Places", icon: MENU_ICONS.places, to: item.to, onClick: () => setOpen(false) },
  ];
  return (
    <span ref={ref} className={"sb__pop" + (open ? " is-open" : "")}>
      <Btn {...item} to={undefined} active={open} menu expanded={open} onClick={() => setOpen((o) => !o)} />
      {open ? (
        <div className="sb__menu">
          <ContextMenu items={items} autoFocus onClose={() => setOpen(false)} />
        </div>
      ) : null}
    </span>
  );
}

type SidebarProps = {
  avatarPreview?: string | null;
  onProfileToggle?: () => void;
  chatOpen?: boolean;
  onChatToggle?: () => void;
  notifOpen?: boolean;
  onNotifToggle?: () => void;
  voiceOpen?: boolean;
  onVoiceToggle?: () => void;
  skyboxOpen?: boolean;
  onSkyboxToggle?: () => void;
  portablesOpen?: boolean;
  onPortablesToggle?: () => void;
  friendsOpen?: boolean;
  onFriendsToggle?: () => void;
  emoteOpen?: boolean;
  onEmoteToggle?: () => void;
  unread?: number;
};

export default function Sidebar({
  avatarPreview,
  onProfileToggle,
  chatOpen,
  onChatToggle,
  notifOpen,
  onNotifToggle,
  voiceOpen,
  onVoiceToggle,
  skyboxOpen,
  onSkyboxToggle,
  portablesOpen,
  onPortablesToggle,
  friendsOpen,
  onFriendsToggle,
  emoteOpen,
  onEmoteToggle,
  unread = 0,
}: SidebarProps) {
  const mic = useBridgeState((s) => s.mic);
  const friends = useBridgeState((s) => s.friends);
  return (
    <div className="sb__stage">
      <nav className="sb" aria-label="Main menu">
        <Tooltip label="More options" side="right">
          <button className="sb__cfg" aria-label="More options" data-sb-linkto="Explorer/Pages/Settings">
            {ICONS.overflow}
          </button>
        </Tooltip>
        <Tooltip label="Profile" side="right">
          <button className="sb__profile" type="button" aria-label="Profile" onClick={onProfileToggle}>
            <Avatar hue={320} size={38} src={avatarPreview || undefined} className="sb__avatar" />
          </button>
        </Tooltip>
        <Tooltip label={unread > 0 ? `Notifications (${unread} unread)` : "Notifications"} side="right">
          <button
            className={"sb__btn" + (notifOpen ? " is-active" : "")}
            type="button"
            aria-label={unread > 0 ? `Notifications (${unread} unread)` : "Notifications"}
            data-sb-panel="notifications"
            onClick={onNotifToggle}
          >
            {ICONS.bell}
            {unread > 0 ? (
              <span className="sb__badge">
                <Badge tone="ruby">{unread > 99 ? "99+" : unread}</Badge>
              </span>
            ) : null}
          </button>
        </Tooltip>

        <div className="sb__group">
          {SIDEBAR_UPPER.map((b) => (
            <span key={b.icon} className="sb__item">
              {b.div ? <span className="sb__divider" /> : null}
              {b.icon === "places" ? <PlacesBtn item={b} /> : <Btn {...b} />}
            </span>
          ))}
        </div>

        <div className="sb__spacer" />

        <div className="sb__group">
          {SIDEBAR_LOWER.map((b) => (
            <span key={b.icon} className="sb__item">
              {b.div ? <span className="sb__divider" /> : null}
              {b.icon === "chat" ? (
                <Btn icon="chat" label="Chat" shortcut={b.shortcut} active={chatOpen} onClick={onChatToggle} />
              ) : b.icon === "voice" ? (
                <Btn {...b} to={undefined} panel="voice" active={voiceOpen} dot={mic.enabled} onClick={onVoiceToggle} />
              ) : b.icon === "skybox" ? (
                <Btn {...b} to={undefined} panel="skybox" active={skyboxOpen} onClick={onSkyboxToggle} />
              ) : b.icon === "wearables" ? (
                <Btn {...b} to={undefined} panel="portables" active={portablesOpen} onClick={onPortablesToggle} />
              ) : b.icon === "emote" ? (
                <Btn icon="emote" label="Emotes" shortcut={b.shortcut} active={emoteOpen} onClick={onEmoteToggle} />
              ) : b.icon === "friends" ? (
                <Btn {...b} to={undefined} panel="friends" active={friendsOpen} notifDot={friends.onlineCount > 0} onClick={onFriendsToggle} />
              ) : (
                <Btn {...b} />
              )}
            </span>
          ))}
        </div>
      </nav>
    </div>
  );
}
