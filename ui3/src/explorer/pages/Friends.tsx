import type { KeyboardEvent, MouseEvent } from "react";
import { useRef, useState } from "react";
import { Avatar, Badge } from "../../atoms/primitives";
import { requestFriendAction, FRIEND_ACTIONS } from "../../data/hooks/friendActions";
import EmptyState from "../../components/EmptyState";
import ProfileCard from "../components/ProfileCard";
import "./friends.css";

type SectionId = "friends" | "requests" | "blocked";
type FriendStatus = "online" | "away" | "offline";

type Friend = {
  name: string;
  tag: string;
  online: boolean;
  status?: FriendStatus;
  where: string;
  hue: number;
  address?: string;
  hasClaimedName?: boolean;
  profilePictureUrl?: string;
};
type Request = {
  name: string;
  tag: string;
  date: string;
  hue: number;
  id?: string;
  address?: string;
  hasClaimedName?: boolean;
  profilePictureUrl?: string;
};
type BlockedUser = {
  name: string;
  tag: string;
  date: string;
  hue: number;
  address?: string;
  hasClaimedName?: boolean;
  profilePictureUrl?: string;
};

const SECTIONS: { id: SectionId; label: string }[] = [
  { id: "friends", label: "Friends" },
  { id: "requests", label: "Requests" },
  { id: "blocked", label: "Blocked" },
];

function Verified() {
  return (
    <svg className="fr__verified" viewBox="0 0 16 16" aria-label="verified">
      <defs>
        <linearGradient id="fr-vrf" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ff2d55" />
          <stop offset="1" stopColor="#c640cd" />
        </linearGradient>
      </defs>
      <path
        d="M8 1l1.7 1.2 2.1-.2 1 1.8 1.9.9-.5 2 .9 1.9-1.6 1.4.1 2.1-2 .6-1.1 1.8-2-.7-2 .7-1.1-1.8-2-.6.1-2.1L1.6 8.6l.9-1.9-.5-2 1.9-.9 1-1.8 2.1.2z"
        fill="url(#fr-vrf)"
      />
      <path d="M5.5 8l1.7 1.7L10.8 6" stroke="#fff" strokeWidth="1.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

type GroupHeaderProps = {
  label: string;
  count: number;
  collapsed?: boolean;
  onToggle?: () => void;
};

function GroupHeader({ label, count, collapsed, onToggle }: GroupHeaderProps) {
  return (
    <button
      type="button"
      className={"fr__group" + (collapsed ? " is-collapsed" : "")}
      aria-expanded={!collapsed}
      onClick={onToggle}
    >
      <span className="fr__caret" aria-hidden="true">&#x25BE;</span>
      {label} ({count})
    </button>
  );
}

type OpenCard = { address: string; name: string; x: number; y: number };

type FriendsProps = {
  initialSection?: SectionId;
  floating?: boolean;
  isGuest?: boolean;
  friends?: Friend[];
  received?: Request[];
  sent?: Request[];
  blocked?: BlockedUser[];
  onClose?: () => void;
};

export default function Friends({
  initialSection = "friends",
  floating = false,
  isGuest = false,
  friends = [],
  received = [],
  sent = [],
  blocked = [],
  onClose,
}: FriendsProps = {}) {
  const [section, setSection] = useState<SectionId>(
    SECTIONS.some((s) => s.id === initialSection) ? initialSection : "friends",
  );
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const onTabKey = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    const d = e.key === "ArrowRight" ? 1 : -1;
    const next = (i + d + SECTIONS.length) % SECTIONS.length;
    const nextSection = SECTIONS[next];
    if (!nextSection) return;
    setSection(nextSection.id);
    tabRefs.current[next]?.focus();
  };
  const [acted, setActed] = useState<Set<string>>(() => new Set());
  const act = (action: string, address?: string) => {
    if (!address) return;
    requestFriendAction(action, address);
    setActed((s) => new Set(s).add(address));
  };
  const remove = (name: string, address?: string) => {
    if (!address) return;
    if (typeof window !== "undefined" && !window.confirm(`Remove ${name} from friends?`)) return;
    act(FRIEND_ACTIONS.DELETE, address);
  };
  const [card, setCard] = useState<OpenCard | null>(null);
  const openCard = (f: { address?: string; name: string }, e: MouseEvent<HTMLButtonElement>) => {
    if (!f.address) return;
    setCard({ address: f.address, name: f.name, x: e.clientX, y: e.clientY });
  };
  const openCardMenu = (f: { address?: string; name: string }, e: MouseEvent<HTMLButtonElement>) => {
    if (e.type === "contextmenu") e.preventDefault();
    openCard(f, e);
  };
  const online = friends.filter((f) => f.online && !acted.has(f.address ?? ""));
  const offline = friends.filter((f) => !f.online && !acted.has(f.address ?? ""));
  const recvList = received.filter((r) => !acted.has(r.address ?? ""));
  const sentList = sent.filter((r) => !acted.has(r.address ?? ""));
  const blockedList = blocked.filter((b) => !acted.has(b.address ?? ""));
  const reqCount = recvList.length + sentList.length;

  const toggle = (key: string) =>
    setCollapsed((c) => ({ ...c, [key]: !c[key] }));

  return (
    <div className={"fr" + (floating ? " fr--floating" : "")}>
      <header className="fr__tabs">
        <div role="tablist" aria-label="Friends sections" style={{ display: "contents" }}>
          {SECTIONS.map((s, i) => (
            <button
              key={s.id}
              role="tab"
              aria-selected={s.id === section}
              tabIndex={s.id === section ? 0 : -1}
              ref={(el) => { tabRefs.current[i] = el; }}
              className={"fr__tab" + (s.id === section ? " is-active" : "")}
              onClick={() => setSection(s.id)}
              onKeyDown={(e) => onTabKey(e, i)}
            >
              {s.label}
              {s.id === "requests" && reqCount > 0 && <Badge>{reqCount}</Badge>}
            </button>
          ))}
        </div>
        <button className="fr__close" aria-label="Close" onClick={onClose}>&#xD7;</button>
      </header>

      <div className="fr__body">
        {isGuest ? (
          <EmptyState
            role="status"
            variant="inline"
            title="Friends aren't available"
            subtitle="Sign in with a wallet to add and manage friends."
          />
        ) : section === "friends" ? (
          <>
            <GroupHeader
              label="Online"
              count={online.length}
              collapsed={collapsed.online}
              onToggle={() => toggle("online")}
            />
            {collapsed.online ? null : online.length === 0 ? (
              <div className="fr__empty">No friends</div>
            ) : (
              online.map((f) => {
                const status: FriendStatus = f.status === "away" ? "away" : "online";
                return (
                  <div className="fr__row" key={f.address || f.name}>
                    <Avatar hue={f.hue} status={status} src={f.profilePictureUrl} size={30} />
                    <button
                      type="button"
                      className="fr__info fr__infobtn"
                      data-sb-linkto="Explorer/Pages/Passport"
                      onClick={(e) => openCard(f, e)}
                      onContextMenu={(e) => openCardMenu(f, e)}
                    >
                      <div className="fr__name u-truncate">
                        {f.name}
                        {f.hasClaimedName && <Verified />}
                        <span className="fr__tag">{f.tag}</span>
                      </div>
                      <div className={"fr__status u-truncate" + (status === "online" ? " fr__status--on" : "")}>
                        {status === "online" ? f.where : "Away"}
                      </div>
                    </button>
                    <div className="fr__actions">
                      <button className="fr__act" title="Remove friend" aria-label="Remove friend" onClick={() => remove(f.name, f.address)}>&#x22EF;</button>
                    </div>
                  </div>
                );
              })
            )}

            <GroupHeader
              label="Offline"
              count={offline.length}
              collapsed={collapsed.offline}
              onToggle={() => toggle("offline")}
            />
            {collapsed.offline ? null : offline.length === 0 ? (
              <div className="fr__empty">No friends</div>
            ) : (
              offline.map((f) => (
                <div className="fr__row" key={f.address || f.name}>
                  <Avatar hue={f.hue} status="offline" src={f.profilePictureUrl} size={30} className="fr__av--off" />
                  <button
                    type="button"
                    className="fr__info fr__infobtn"
                    data-sb-linkto="Explorer/Pages/Passport"
                    onClick={(e) => openCard(f, e)}
                    onContextMenu={(e) => openCardMenu(f, e)}
                  >
                    <div className="fr__name u-truncate">
                      {f.name}
                      {f.hasClaimedName && <Verified />}
                      <span className="fr__tag">{f.tag}</span>
                    </div>
                    <div className="fr__status u-truncate">Offline</div>
                  </button>
                  <div className="fr__actions">
                    <button className="fr__act" title="Remove friend" aria-label="Remove friend" onClick={() => remove(f.name, f.address)}>&#x22EF;</button>
                  </div>
                </div>
              ))
            )}
          </>
        ) : section === "requests" ? (
          <>
            <GroupHeader
              label="Received"
              count={recvList.length}
              collapsed={collapsed.received}
              onToggle={() => toggle("received")}
            />
            {collapsed.received ? null : recvList.length === 0 ? (
              <div className="fr__empty">No requests</div>
            ) : (
              recvList.map((r) => (
                <div className="fr__row" key={r.id || r.address || r.name}>
                  <Avatar hue={r.hue} src={r.profilePictureUrl} size={30} />
                  <div className="fr__info">
                    <div className="fr__name u-truncate">
                      {r.name}
                      {r.hasClaimedName && <Verified />}
                      <span className="fr__tag">{r.tag}</span>
                    </div>
                    <div className="fr__status u-truncate">Wants to be your friend &#xB7; {r.date}</div>
                  </div>
                  <div className="fr__actions">
                    <button className="fr__accept" title="Accept" aria-label="Accept" onClick={() => act(FRIEND_ACTIONS.ACCEPT, r.address)}>&#x2713;</button>
                    <button className="fr__reject" title="Reject" aria-label="Reject" onClick={() => act(FRIEND_ACTIONS.REJECT, r.address)}>&#xD7;</button>
                  </div>
                </div>
              ))
            )}

            <GroupHeader
              label="Sent"
              count={sentList.length}
              collapsed={collapsed.sent}
              onToggle={() => toggle("sent")}
            />
            {collapsed.sent ? null : sentList.length === 0 ? (
              <div className="fr__empty">No requests</div>
            ) : (
              sentList.map((r) => (
                <div className="fr__row" key={r.id || r.address || r.name}>
                  <Avatar hue={r.hue} src={r.profilePictureUrl} size={30} />
                  <div className="fr__info">
                    <div className="fr__name u-truncate">
                      {r.name}
                      {r.hasClaimedName && <Verified />}
                      <span className="fr__tag">{r.tag}</span>
                    </div>
                    <div className="fr__status u-truncate">Request sent &#xB7; {r.date}</div>
                  </div>
                  <div className="fr__actions">
                    <button className="fr__cancel" onClick={() => act(FRIEND_ACTIONS.CANCEL, r.address)}>Cancel</button>
                  </div>
                </div>
              ))
            )}
          </>
        ) : (
          <>
            <GroupHeader
              label="Blocked"
              count={blockedList.length}
              collapsed={collapsed.blocked}
              onToggle={() => toggle("blocked")}
            />
            {collapsed.blocked ? null : blockedList.length === 0 ? (
              <EmptyState
                role="status"
                variant="inline"
                icon={<span style={{ fontSize: 26 }}>&#x2298;</span>}
                iconWash
                title="No Blocked Accounts"
                subtitle="If you block someone, you won't see each other in-world or exchange messages, and their name and messages are hidden in public chats."
              />
            ) : (
              blockedList.map((b) => (
                <div className="fr__row" key={b.address || b.name}>
                  <Avatar hue={b.hue} src={b.profilePictureUrl} size={30} className="fr__av--off" />
                  <div className="fr__info">
                    <div className="fr__name u-truncate">
                      {b.name}
                      {b.hasClaimedName && <Verified />}
                      <span className="fr__tag">{b.tag}</span>
                    </div>
                    <div className="fr__status u-truncate">Blocked &#xB7; {b.date}</div>
                  </div>
                  <div className="fr__actions">
                    <button className="fr__unblock" onClick={() => act(FRIEND_ACTIONS.UNBLOCK, b.address)}>Unblock</button>
                  </div>
                </div>
              ))
            )}
          </>
        )}
      </div>

      {card && (
        <ProfileCard
          user={{ address: card.address, name: card.name }}
          x={card.x}
          y={card.y}
          onClose={() => setCard(null)}
        />
      )}
    </div>
  );
}
