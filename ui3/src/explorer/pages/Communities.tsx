import type { CSSProperties, ReactNode } from "react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import ExploreChrome, { type TabId } from "../frames/ExploreChrome";
import ConfirmDialog from "../components/ConfirmDialog";
import { SectionCard } from "../../components/Surface";
import ContentStatus from "../../components/ContentStatus";
import { Avatar, fmt, hueFromSeed } from "../../atoms/primitives";
import { asset } from "../../asset";
import "./social.css";

export type SocialCommunity = {
  id: string;
  name: string;
  description?: string;
  thumbnailUrl?: string | null;
  visibility?: string | null;
  privacy?: string;
  isLive?: boolean;
  membersCount: number;
  role?: string | null;
  ownerName?: string | null;
};

export type SocialFriend = {
  address: string;
  name: string;
  online: boolean;
  status?: "online" | "away" | "offline";
  where: string;
  hue?: number;
  profilePictureUrl?: string;
};

export type SocialEvent = {
  id: string;
  name: string;
  image?: string | null;
  startAt?: string | null;
  attendees: number;
  live?: boolean;
};

export type SocialPerson = { address: string; name: string; coords: string; picture?: string };

export type SocialMember = {
  memberAddress: string;
  name: string;
  role: string;
  profilePictureUrl?: string;
  hasClaimedName?: boolean;
};

export type SocialPost = {
  id: string;
  authorAddress: string;
  authorName: string;
  authorProfilePictureUrl?: string;
  authorHasClaimedName?: boolean;
  content: string;
  createdAt: string | null;
  likesCount: number;
};

export type SocialPlace = {
  id: string;
  title?: string;
  description?: string;
  image?: string | null;
  location?: string;
  addedBy: string;
  addedAt: string | null;
};

export type SocialDetail = {
  id: string;
  community: SocialCommunity | null;
  members: SocialMember[];
  posts: SocialPost[];
  places: SocialPlace[];
  isPending?: boolean;
  isError?: boolean;
  postsPending?: boolean;
  placesPending?: boolean;
};

export type SocialMembership = {
  joined: boolean;
  requested?: boolean;
  pending?: "join" | "leave" | null;
  error?: string | null;
};

export type SectionId = "home" | "explore" | "friends" | "nearby";
export type ChannelId = "summary" | "general" | "announcements" | "hangouts" | "members";

type VarStyle = CSSProperties & { [k: `--${string}`]: string | number };

const isHttpUrl = (s: unknown): s is string => typeof s === "string" && /^https?:\/\//i.test(s);
const cap = (s: string): string => (s ? s.charAt(0).toUpperCase() + s.slice(1) : "");

function isMember(c: SocialCommunity): boolean {
  return Boolean(c.role) && c.role !== "none";
}

function visLabel(c: SocialCommunity): string {
  if (c.visibility === "unlisted") return "Unlisted";
  return c.privacy === "private" ? "Private" : "Public";
}

function shortAddress(addr?: string | null): string {
  return `${(addr || "").slice(0, 6)}\u{2026}${(addr || "").slice(-4)}`;
}

export function relativeDate(iso?: string | null, now: number = Date.now()): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const n = new Date(now);
  const months = (n.getFullYear() - d.getFullYear()) * 12 + n.getMonth() - d.getMonth() - (n.getDate() < d.getDate() ? 1 : 0);
  if (months >= 12) {
    const years = Math.floor(months / 12);
    return years === 1 ? "1 year ago" : `${years} years ago`;
  }
  if (months >= 1) return months === 1 ? "1 month ago" : `${months} months ago`;
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

export function fullDate(iso?: string | null): string | undefined {
  if (!iso) return undefined;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return undefined;
  return d.toLocaleString([], { dateStyle: "full", timeStyle: "short" });
}

export function oldestFirst(posts: SocialPost[]): SocialPost[] {
  return posts
    .map((post, index) => ({ post, index, at: Date.parse(post.createdAt ?? "") || 0 }))
    .sort((a, b) => a.at - b.at || b.index - a.index)
    .map((row) => row.post);
}

const ICONS: Record<string, ReactNode> = {
  home: <path d="M4 11.2 12 4l8 7.2V20a1 1 0 0 1-1 1h-4.5v-6h-5v6H5a1 1 0 0 1-1-1v-8.8Z" />,
  explore: <><circle cx="11" cy="11" r="6.5" /><path d="m20 20-4.2-4.2" /></>,
  friends: <><circle cx="9" cy="8" r="3.2" /><path d="M3 20c0-3.3 2.7-5.5 6-5.5s6 2.2 6 5.5" /><path d="M16 5.2a3.2 3.2 0 0 1 0 5.6M17 14.7c2.4.5 4 2.4 4 5.3" /></>,
  nearby: <><path d="M9 4 3.5 6.2v13.3L9 17.3l6 2.4 5.5-2.2V4.2L15 6.4 9 4Z" /><circle cx="12" cy="9.6" r="1.9" /><path d="M8.7 15.1c.5-1.9 1.7-2.9 3.3-2.9s2.8 1 3.3 2.9" /></>,
  calendar: <><rect x="3" y="4.5" width="18" height="16" rx="2.5" /><path d="M3 9h18M8 2.5v4M16 2.5v4" /></>,
  chat: <path d="M20 3H4a1 1 0 0 0-1 1v12a1 1 0 0 0 1 1h4l4 4 4-4h4a1 1 0 0 0 1-1V4a1 1 0 0 0-1-1Z" />,
  bell: <><path d="M6 16V11a6 6 0 1 1 12 0v5l1.5 2h-15L6 16Z" /><path d="M10 20.5a2 2 0 0 0 4 0" /></>,
  globe: <><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" /></>,
  plus: <path d="M12 5v14M5 12h14" />,
  arrow: <path d="M5 12h14M13 6l6 6-6 6" />,
  link: <><rect x="9" y="9" width="11" height="11" rx="2" /><path d="M5 15V5a2 2 0 0 1 2-2h8" /></>,
};

function Icon({ name, size = 20 }: { name: string; size?: number }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {ICONS[name]}
    </svg>
  );
}

function Verified() {
  return (
    <svg className="soc__verified" viewBox="0 0 24 24" width="13" height="13" aria-label="verified">
      <path d="M12 2l2.4 1.8 3-.3 1 2.8 2.6 1.5-.9 2.9.9 2.9-2.6 1.5-1 2.8-3-.3L12 22l-2.4-1.8-3 .3-1-2.8L3 16.2l.9-2.9L3 10.4l2.6-1.5 1-2.8 3 .3L12 2z" fill="#4d8dff" />
      <path d="M8.5 12l2.2 2.2 4.5-4.5" stroke="#fff" strokeWidth="1.8" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

const SECTIONS: { id: SectionId; label: string; icon: string }[] = [
  { id: "home", label: "Home", icon: "home" },
  { id: "explore", label: "Explore communities", icon: "explore" },
  { id: "friends", label: "Friends", icon: "friends" },
  { id: "nearby", label: "Nearby", icon: "nearby" },
];

const CHANNELS: { id: Exclude<ChannelId, "summary">; label: string; icon: string }[] = [
  { id: "general", label: "General", icon: "chat" },
  { id: "announcements", label: "Announcements", icon: "bell" },
  { id: "hangouts", label: "Hangouts", icon: "globe" },
  { id: "members", label: "Members", icon: "friends" },
];

function Cover({ community, className }: { community: SocialCommunity; className: string }) {
  const style: VarStyle = { "--hue": hueFromSeed(community.id || community.name) };
  return (
    <span className={className} style={style}>
      {isHttpUrl(community.thumbnailUrl)
        ? <img src={community.thumbnailUrl} alt="" loading="lazy" />
        : <span aria-hidden="true">{community.name.trim().slice(0, 2).toUpperCase()}</span>}
    </span>
  );
}

type JoinProps = {
  community: SocialCommunity;
  membership: SocialMembership;
  onJoin?: (community: SocialCommunity) => void;
  onLeave?: (community: SocialCommunity) => void;
};

function JoinControl({ community, membership, onJoin, onLeave }: JoinProps) {
  const [confirmLeave, setConfirmLeave] = useState(false);
  const isPublic = community.privacy !== "private";
  const busy = Boolean(membership.pending);

  let label;
  if (membership.joined) label = membership.pending === "leave" ? "Leaving\u{2026}" : "Joined";
  else if (membership.requested) label = "Requested";
  else if (membership.pending === "join") label = isPublic ? "Joining\u{2026}" : "Requesting\u{2026}";
  else label = isPublic ? "Join community" : "Request to join";

  return (
    <div className="soc__joinwrap">
      <button
        className={"soc__join" + (membership.joined ? " is-joined" : "")}
        type="button"
        disabled={busy || (membership.requested && !membership.joined)}
        onClick={() => (membership.joined ? setConfirmLeave(true) : onJoin?.(community))}
      >
        {label}
      </button>
      {membership.error ? <span className="soc__joinerr" role="alert">{membership.error}</span> : null}
      {confirmLeave && createPortal(
        <ConfirmDialog
          title="Leave community?"
          body={`Are you sure you want to leave ${community.name}?`}
          confirmLabel="Leave community"
          cancelLabel="Stay"
          danger
          onConfirm={() => { setConfirmLeave(false); onLeave?.(community); }}
          onCancel={() => setConfirmLeave(false)}
        />,
        document.body,
      )}
    </div>
  );
}

function MineList({ mine, onOpen }: { mine: SocialCommunity[]; onOpen: (id: string) => void }) {
  return (
    <ul className="soc__mine">
      {mine.map((c) => (
        <li key={c.id}>
          <button className="soc__minerow" type="button" onClick={() => onOpen(c.id)}>
            <Cover community={c} className="soc__thumb" />
            <span className="soc__rowtext">
              <strong className="u-truncate">{c.name}</strong>
              <small>{fmt(c.membersCount)} members &#xB7; {cap(c.role || "member")}</small>
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

type HomeProps = {
  friends: readonly SocialFriend[];
  friendsLoading: boolean;
  isGuest: boolean;
  communities: SocialCommunity[];
  mine: SocialCommunity[];
  communitiesLoading: boolean;
  communitiesError: boolean;
  onRetry?: () => void;
  events: SocialEvent[];
  eventsLoading: boolean;
  onSection: (id: SectionId) => void;
  onOpen: (id: string) => void;
  onMessage?: (address: string) => void;
  onCreate?: () => void;
};

function Home(p: HomeProps) {
  const people = useMemo(
    () => [...p.friends].sort((a, b) => Number(b.online) - Number(a.online) || a.name.localeCompare(b.name)),
    [p.friends],
  );
  const online = p.friends.filter((f) => f.online).length;
  const groups = p.mine.length ? p.mine : p.communities;
  return (
    <div className="soc__home">
      <header className="soc__hero-text">
        <div>
          <h1>A little closer, wherever you are.</h1>
          <p>Your people. Your communities. Your next adventure.</p>
        </div>
        <button className="soc__outline" type="button" data-sb-linkto="Explorer/Pages/Friends">
          <Icon name="plus" size={16} />Add a friend
        </button>
      </header>
      <div className="soc__columns">
        <SectionCard
          title="Friends"
          titleId="soc-home-friends"
          icon={<Icon name="friends" size={17} />}
          action={<button className="soc__cardlink" type="button" onClick={() => p.onSection("friends")}>Open friends <span aria-hidden="true">&#x2197;</span></button>}
          footer={<button className="soc__cardlink" type="button" onClick={() => p.onSection("nearby")}><Icon name="nearby" size={16} />Find friends out exploring</button>}
        >
          {p.isGuest ? (
            <div className="soc__empty">
              <h3>Good company starts here.</h3>
              <p>Sign in to find your friends, see who&#x2019;s around, and pick up a conversation.</p>
            </div>
          ) : (
            <>
              <div className="soc__note">
                <span className={"soc__dot" + (online ? " is-online" : "")} />
                {p.friendsLoading ? "Connecting friends\u{2026}" : `${online} online`}
                <button className="soc__cardlink" type="button" data-sb-linkto="Explorer/Pages/Friends">Requests</button>
              </div>
              {people.slice(0, 5).map((f) => (
                <button className="soc__person" type="button" key={f.address || f.name} aria-label={`Message ${f.name}`} onClick={() => p.onMessage?.(f.address)} data-sb-linkto="Explorer/Frames/Chat">
                  <Avatar size={38} name={f.name} seed={f.address || f.name} hue={f.hue} src={isHttpUrl(f.profilePictureUrl) ? f.profilePictureUrl : undefined} status={f.status ?? (f.online ? "online" : "offline")} />
                  <span className="soc__rowtext"><strong className="u-truncate">{f.name}</strong><small className="u-truncate">{f.where}</small></span>
                  <Icon name="chat" size={16} />
                </button>
              ))}
              {!p.friendsLoading && !people.length && (
                <div className="soc__empty">
                  <h3>Make room for your people.</h3>
                  <p>Add a friend to start chatting and exploring together.</p>
                </div>
              )}
            </>
          )}
        </SectionCard>

        <SectionCard
          title="Communities"
          titleId="soc-home-communities"
          icon={<Icon name="chat" size={17} />}
          action={<button className="soc__cardlink" type="button" onClick={() => p.onSection("explore")}>Discover <span aria-hidden="true">&#x2197;</span></button>}
          footer={<>
            <button className="soc__cardlink" type="button" onClick={() => p.onSection("explore")}><Icon name="friends" size={16} />Your communities</button>
            <button className="soc__cardlink" type="button" aria-label="Create a community" onClick={p.onCreate} data-sb-linkto={p.onCreate ? undefined : "Explorer/Components/CommunityCreate"}><Icon name="plus" size={16} /></button>
          </>}
        >
          <div className="soc__note">
            {p.mine.length ? "Your communities" : "Find your kind of people"}
            <button className="soc__cardlink" type="button" data-sb-linkto="Explorer/Components/Notifications">Invitations</button>
          </div>
          {p.communitiesLoading && !groups.length && <p className="soc__muted" role="status">Loading communities&#x2026;</p>}
          {p.communitiesError && !groups.length && <ContentStatus message="Couldn't load communities." onRetry={p.onRetry} />}
          {groups.slice(0, 4).map((c) => (
            <button className="soc__group" type="button" key={c.id} onClick={() => p.onOpen(c.id)}>
              <Cover community={c} className="soc__thumb" />
              <span className="soc__rowtext">
                <strong className="u-truncate">{c.name}</strong>
                <small>{fmt(c.membersCount)} members{c.privacy === "private" ? " \u{B7} Private" : ""}</small>
                {c.description ? <span className="soc__clamp">{c.description}</span> : null}
              </span>
            </button>
          ))}
          {!p.communitiesLoading && !p.communitiesError && !groups.length && (
            <div className="soc__empty">
              <h3>A place to belong.</h3>
              <p>Start a community and bring your people together.</p>
            </div>
          )}
        </SectionCard>

        <SectionCard
          title="Events"
          titleId="soc-home-events"
          icon={<Icon name="calendar" size={17} />}
          action={<button className="soc__cardlink" type="button" data-sb-linkto="Explorer/Pages/Events">All events <span aria-hidden="true">&#x2197;</span></button>}
        >
          {p.eventsLoading && !p.events.length && <p className="soc__muted" role="status">Loading events&#x2026;</p>}
          {p.events.slice(0, 3).map((e) => {
            const start = e.startAt ? new Date(e.startAt) : null;
            const valid = start && !Number.isNaN(start.getTime()) ? start : null;
            return (
              <button className="soc__event" type="button" key={e.id} data-sb-linkto="Explorer/Pages/Events">
                <span className="soc__eventart" style={{ "--hue": hueFromSeed(e.id) } as VarStyle}>
                  {e.image ? <img src={e.image} alt="" loading="lazy" /> : null}
                </span>
                {valid && (
                  <span className="soc__eventday">
                    <b>{valid.getDate()}</b>
                    {valid.toLocaleDateString([], { month: "short" })}
                  </span>
                )}
                <span className="soc__rowtext">
                  <small>{e.live ? "Live now" : valid ? valid.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }) : ""}</small>
                  <strong>{e.name}</strong>
                  <small>{fmt(e.attendees)} going</small>
                </span>
              </button>
            );
          })}
          {!p.eventsLoading && !p.events.length && (
            <div className="soc__empty">
              <h3>Nothing on the calendar yet.</h3>
              <p>Upcoming events show up here.</p>
            </div>
          )}
        </SectionCard>
      </div>
    </div>
  );
}

type ExploreProps = {
  communities: SocialCommunity[];
  mine: SocialCommunity[];
  query: string;
  isLoading: boolean;
  isError: boolean;
  onRetry?: () => void;
  onOpen: (id: string) => void;
};

function Explore({ communities, mine, query, isLoading, isError, onRetry, onOpen }: ExploreProps) {
  const q = query.trim().toLowerCase();
  const visible = useMemo(
    () => communities.filter((c) => !q || c.name.toLowerCase().includes(q) || (c.ownerName || "").toLowerCase().includes(q)),
    [communities, q],
  );
  return (
    <div className="soc__discover">
      {!q && (
        <div className="soc__hero">
          <img src={asset("assets/login-avatars.png")} alt="" />
          <div>
            <h1>Find your kind<br />of people.</h1>
            <p>Good company. Shared worlds.<br />A conversation worth coming back to.</p>
          </div>
        </div>
      )}
      {!q && mine.length > 0 && (
        <section aria-labelledby="soc-explore-mine">
          <h2 className="soc__heading" id="soc-explore-mine">Your communities</h2>
          <MineList mine={mine} onOpen={onOpen} />
        </section>
      )}
      <section aria-labelledby="soc-explore-all">
        <h2 className="soc__heading" id="soc-explore-all">Explore communities</h2>
        {isLoading ? (
          <div className="soc__cards" aria-busy="true">
            {Array.from({ length: 6 }).map((_, i) => (
              <div className="soc__card is-loading" key={i}>
                <span className="soc__cover" style={{ "--hue": (i * 47) % 360 } as VarStyle} />
                <span className="soc__cardbody"><strong>Loading&#x2026;</strong></span>
              </div>
            ))}
          </div>
        ) : isError ? (
          <ContentStatus message="Couldn't load communities." onRetry={onRetry} />
        ) : visible.length === 0 ? (
          <p className="soc__muted">{q ? "No communities match your search." : "No communities to show."}</p>
        ) : (
          <div className="soc__cards">
            {visible.map((c) => (
              <button className="soc__card" type="button" key={c.id} onClick={() => onOpen(c.id)}>
                <Cover community={c} className="soc__cover" />
                {c.isLive ? <span className="soc__live">&#x25CF; LIVE</span> : null}
                <span className="soc__cardbody">
                  <strong>{c.name}</strong>
                  <span className="soc__clamp">{c.description}</span>
                  <small><i />{fmt(c.membersCount)} members{isMember(c) ? " \u{B7} Joined" : ""}</small>
                  <span className="soc__cta">Take a look <span aria-hidden="true">&#x2192;</span></span>
                </span>
              </button>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function FriendsView({ friends, loading, isGuest, onMessage }: { friends: readonly SocialFriend[]; loading: boolean; isGuest: boolean; onMessage?: (address: string) => void }) {
  const online = friends.filter((f) => f.online);
  const offline = friends.filter((f) => !f.online);
  if (isGuest) {
    return <div className="soc__page"><div className="soc__empty"><h3>Your friends, together.</h3><p>Sign in to see your Decentraland friends and chat.</p></div></div>;
  }
  if (loading) return <div className="soc__page"><ContentStatus pending message="Connecting friends&hellip;" /></div>;
  const group = (label: string, list: SocialFriend[]) => list.length > 0 && (
    <section aria-label={label}>
      <h2 className="soc__heading">{label} <small>{list.length}</small></h2>
      <ul className="soc__list">
        {list.map((f) => (
          <li className="soc__member" key={f.address || f.name}>
            <Avatar size={40} name={f.name} seed={f.address || f.name} hue={f.hue} src={isHttpUrl(f.profilePictureUrl) ? f.profilePictureUrl : undefined} status={f.status ?? (f.online ? "online" : "offline")} />
            <span className="soc__rowtext"><strong className="u-truncate">{f.name}</strong><small className="u-truncate">{f.where}</small></span>
            <button className="soc__outline" type="button" aria-label={`Message ${f.name}`} onClick={() => onMessage?.(f.address)} data-sb-linkto="Explorer/Frames/Chat"><Icon name="chat" size={16} />Message</button>
          </li>
        ))}
      </ul>
    </section>
  );
  return (
    <div className="soc__page">
      <header className="soc__pagehead">
        <div><h1>Friends</h1><p>{online.length} online &#xB7; {friends.length} in total</p></div>
        <button className="soc__outline" type="button" data-sb-linkto="Explorer/Pages/Friends">Requests &amp; blocked</button>
      </header>
      {friends.length === 0 ? (
        <div className="soc__empty"><h3>Make room for your people.</h3><p>Add a friend to start chatting and exploring together.</p></div>
      ) : <>{group("Online", online)}{group("Offline", offline)}</>}
    </div>
  );
}

type NearbyProps = {
  nearby: SocialPerson[];
  friends: readonly SocialFriend[];
  selfAddress?: string | null;
  sent: ReadonlySet<string>;
  onAddFriend: (address: string) => void;
};

function NearbyView({ nearby, friends, selfAddress, sent, onAddFriend }: NearbyProps) {
  const friendSet = useMemo(() => new Set(friends.map((f) => f.address.toLowerCase())), [friends]);
  const around = nearby.filter((person) => person.address.toLowerCase() !== selfAddress?.toLowerCase());
  const exploring = friends.filter((f) => f.status ? f.status === "online" : f.online);
  return (
    <div className="soc__page">
      <header className="soc__pagehead">
        <div><h1>Nearby</h1><p>The people sharing this place with you, and friends out exploring.</p></div>
        <button className="soc__outline" type="button" data-sb-linkto="Explorer/Pages/Map"><Icon name="nearby" size={16} />Open the map</button>
      </header>
      <section aria-label="Around you">
        <h2 className="soc__heading">Around you <small>{around.length}</small></h2>
        {around.length === 0 ? <p className="soc__muted">Nobody else is here right now.</p> : (
          <ul className="soc__list">
            {around.map((person) => {
              const address = person.address.toLowerCase();
              return (
                <li className="soc__member" key={person.address}>
                  <Avatar size={40} name={person.name} seed={person.address} src={isHttpUrl(person.picture) ? person.picture : undefined} />
                  <span className="soc__rowtext"><strong className="u-truncate">{person.name || shortAddress(person.address)}</strong><small>At {person.coords}</small></span>
                  {friendSet.has(address) ? <span className="soc__tag">Friend</span>
                    : sent.has(address) ? <span className="soc__tag">Requested</span>
                    : <button className="soc__outline" type="button" onClick={() => onAddFriend(person.address)}><Icon name="plus" size={16} />Add friend</button>}
                </li>
              );
            })}
          </ul>
        )}
      </section>
      <section aria-label="Friends out exploring">
        <h2 className="soc__heading">Friends out exploring <small>{exploring.length}</small></h2>
        {exploring.length === 0 ? <p className="soc__muted">None of your friends are in world right now.</p> : (
          <ul className="soc__list">
            {exploring.map((f) => (
              <li className="soc__member" key={f.address || f.name}>
                <Avatar size={40} name={f.name} seed={f.address || f.name} hue={f.hue} src={isHttpUrl(f.profilePictureUrl) ? f.profilePictureUrl : undefined} status="online" />
                <span className="soc__rowtext"><strong className="u-truncate">{f.name}</strong><small className="u-truncate">{f.where}</small></span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function Message({ post, now }: { post: SocialPost; now?: number }) {
  return (
    <article className="soc__message" title={fullDate(post.createdAt)}>
      <Avatar size={38} name={post.authorName} seed={post.authorAddress} src={isHttpUrl(post.authorProfilePictureUrl) ? post.authorProfilePictureUrl : undefined} />
      <div>
        <header>
          <strong>{post.authorName || shortAddress(post.authorAddress)}</strong>
          {post.authorHasClaimedName && <Verified />}
          <time dateTime={post.createdAt ?? undefined}>{relativeDate(post.createdAt, now)}</time>
        </header>
        <p>{post.content}</p>
      </div>
    </article>
  );
}

function Welcome({ mark, title, text }: { mark: ReactNode; title: string; text: string }) {
  return (
    <div className="soc__welcome">
      <div className="soc__welcomeicon" aria-hidden="true">{mark}</div>
      <h1>{title}</h1>
      <p>{text}</p>
    </div>
  );
}

function Announcements({ detail, now }: { detail: SocialDetail; now?: number }) {
  const scroller = useRef<HTMLDivElement>(null);
  const thread = useRef<HTMLDivElement>(null);
  const followTail = useRef(true);
  const posts = useMemo(() => oldestFirst(detail.posts), [detail.posts]);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return undefined;
    const pin = () => { if (followTail.current) el.scrollTop = el.scrollHeight; };
    followTail.current = true;
    pin();
    if (!thread.current || typeof ResizeObserver === "undefined") return undefined;
    const observer = new ResizeObserver(pin);
    observer.observe(thread.current);
    return () => observer.disconnect();
  }, [posts, detail.id]);
  return (
    <>
      <div
        className="soc__messages"
        ref={scroller}
        role="log"
        aria-label="Announcements"
        tabIndex={0}
        onScroll={(e) => {
          const el = e.currentTarget;
          followTail.current = el.scrollHeight - el.clientHeight - el.scrollTop < 40;
        }}
      >
        <div className="soc__thread" ref={thread}>
          <Welcome mark={<Icon name="bell" size={28} />} title="From your community." text="Announcements shared with Decentraland." />
          {detail.postsPending ? <ContentStatus pending message="Loading announcements&hellip;" />
            : posts.length === 0 ? <p className="soc__muted">No announcements yet.</p>
            : posts.map((post) => <Message key={post.id} post={post} now={now} />)}
        </div>
      </div>
      <p className="soc__footnote">Only moderators can post here.</p>
    </>
  );
}

function Hangouts({ detail, name, now }: { detail: SocialDetail; name: string; now?: number }) {
  return (
    <div className="soc__page">
      <header className="soc__pagehead"><div><h1>Hangouts</h1><p>Places where {name} gets together.</p></div></header>
      {detail.placesPending ? <ContentStatus pending message="Loading shared places&hellip;" />
        : detail.places.length === 0 ? <p className="soc__muted">No places shared yet.</p>
        : (
          <ul className="soc__hangouts">
            {detail.places.map((place) => (
              <li className="soc__hangout" key={place.id}>
                <span className="soc__hangoutart" style={{ "--hue": hueFromSeed(place.id) } as VarStyle}>
                  {place.image ? <img src={place.image} alt="" loading="lazy" /> : null}
                </span>
                <span className="soc__rowtext">
                  <strong className="u-truncate" title={place.title || place.id}>{place.title || place.id}</strong>
                  {place.description ? <span className="soc__clamp">{place.description}</span> : null}
                  <small title={fullDate(place.addedAt)}>{place.location ? `${place.location} \u{B7} ` : ""}Added by {shortAddress(place.addedBy)}{place.addedAt ? ` \u{B7} ${relativeDate(place.addedAt, now)}` : ""}</small>
                </span>
                <button className="soc__primary" type="button" data-sb-linkto="Explorer/Pages/Places">Open Places <span aria-hidden="true">&#x2197;</span></button>
              </li>
            ))}
          </ul>
        )}
    </div>
  );
}

type MembersProps = {
  detail: SocialDetail;
  name: string;
  friends: readonly SocialFriend[];
  selfAddress?: string | null;
  sent: ReadonlySet<string>;
  onAddFriend: (address: string) => void;
};

function Members({ detail, name, friends, selfAddress, sent, onAddFriend }: MembersProps) {
  const [query, setQuery] = useState("");
  const friendSet = useMemo(() => new Set(friends.map((f) => f.address.toLowerCase())), [friends]);
  const q = query.trim().toLowerCase();
  const members = detail.members.filter((m) => !q || m.name.toLowerCase().includes(q) || m.memberAddress.toLowerCase().includes(q));
  return (
    <div className="soc__page">
      <header className="soc__pagehead">
        <div><h1>Members</h1><p>Everyone in {name}.</p></div>
        <input className="soc__search" type="search" aria-label="Search members" placeholder="Search member or name" value={query} onChange={(e) => setQuery(e.target.value)} />
      </header>
      {detail.isPending ? <ContentStatus pending message="Loading members&hellip;" />
        : members.length === 0 ? <p className="soc__muted">{q ? "No members match your search." : "No members to show."}</p>
        : (
          <ul className="soc__list soc__list--split">
            {members.map((m) => {
              const address = m.memberAddress.toLowerCase();
              return (
                <li className="soc__member" key={m.memberAddress || m.name}>
                  <Avatar size={40} name={m.name} seed={m.memberAddress || m.name} src={isHttpUrl(m.profilePictureUrl) ? m.profilePictureUrl : undefined} />
                  <span className="soc__rowtext">
                    <strong><span className="u-truncate">{m.name || shortAddress(m.memberAddress)}</span>{m.hasClaimedName && <Verified />}</strong>
                    <small>{cap((m.role || "member").toLowerCase())}</small>
                  </span>
                  {address === selfAddress?.toLowerCase() ? <span className="soc__tag">You</span>
                    : friendSet.has(address) ? <span className="soc__tag">Friend</span>
                    : sent.has(address) ? <span className="soc__tag">Requested</span>
                    : <button className="soc__outline" type="button" onClick={() => onAddFriend(m.memberAddress)}><Icon name="plus" size={16} />Add friend</button>}
                </li>
              );
            })}
          </ul>
        )}
    </div>
  );
}

type SummaryProps = {
  detail: SocialDetail;
  community: SocialCommunity;
  membership: SocialMembership;
  onJoin?: (community: SocialCommunity) => void;
  onLeave?: (community: SocialCommunity) => void;
  onCopyLink?: (id: string) => void;
  onChannel: (channel: ChannelId) => void;
  now?: number;
};

function Summary({ detail, community, membership, onJoin, onLeave, onCopyLink, onChannel, now }: SummaryProps) {
  const [copied, setCopied] = useState(false);
  const latest = useMemo(() => oldestFirst(detail.posts).at(-1), [detail.posts]);
  useEffect(() => {
    if (!copied) return undefined;
    const timer = setTimeout(() => setCopied(false), 1200);
    return () => clearTimeout(timer);
  }, [copied]);
  return (
    <div className="soc__summary">
      <Cover community={community} className="soc__banner" />
      <div className="soc__summaryhead">
        <div>
          <h1>{community.name}</h1>
          <p>{community.description}</p>
        </div>
        <div className="soc__summaryactions">
          <JoinControl community={community} membership={membership} onJoin={onJoin} onLeave={onLeave} />
          <button className="soc__outline" type="button" onClick={() => { onCopyLink?.(community.id); setCopied(true); }}>
            <Icon name="link" size={16} />{copied ? "Copied!" : "Copy link"}
          </button>
        </div>
      </div>
      <dl className="soc__facts">
        <div><dt>Members</dt><dd>{fmt(community.membersCount)}</dd></div>
        <div><dt>Privacy</dt><dd>{visLabel(community)}</dd></div>
        <div><dt>Your role</dt><dd>{membership.joined ? cap(community.role && community.role !== "none" ? community.role : "member") : "Not a member"}</dd></div>
      </dl>
      <div className="soc__columns soc__columns--two">
        <SectionCard
          title="Latest announcement"
          titleId="soc-summary-latest"
          icon={<Icon name="bell" size={17} />}
          action={<button className="soc__cardlink" type="button" onClick={() => onChannel("announcements")}>All announcements <span aria-hidden="true">&#x2192;</span></button>}
        >
          {detail.postsPending ? <p className="soc__muted" role="status">Loading announcements&#x2026;</p>
            : latest ? <Message post={latest} now={now} />
            : <p className="soc__muted">No announcements yet.</p>}
        </SectionCard>
        <SectionCard
          title="Its places"
          titleId="soc-summary-places"
          icon={<Icon name="globe" size={17} />}
          action={<button className="soc__cardlink" type="button" onClick={() => onChannel("hangouts")}>All hangouts <span aria-hidden="true">&#x2192;</span></button>}
        >
          {detail.placesPending ? <p className="soc__muted" role="status">Loading shared places&#x2026;</p>
            : detail.places.length === 0 ? <p className="soc__muted">No places shared yet.</p>
            : detail.places.slice(0, 3).map((place) => (
              <button className="soc__group" type="button" key={place.id} onClick={() => onChannel("hangouts")}>
                <span className="soc__thumb" style={{ "--hue": hueFromSeed(place.id) } as VarStyle}>
                  {place.image ? <img src={place.image} alt="" loading="lazy" /> : null}
                </span>
                <span className="soc__rowtext">
                  <strong className="u-truncate">{place.title || place.id}</strong>
                  <small className="u-truncate">{place.location || `Added by ${shortAddress(place.addedBy)}`}</small>
                </span>
              </button>
            ))}
        </SectionCard>
      </div>
      <button className="soc__chatlink" type="button" onClick={() => onChannel("general")}>
        <Icon name="chat" />
        <span className="soc__rowtext"><strong>Join the conversation</strong><small>General is where {community.name} talks.</small></span>
        <Icon name="arrow" size={18} />
      </button>
    </div>
  );
}

function General({ community, membership, onJoin, onLeave }: Pick<SummaryProps, "community" | "membership" | "onJoin" | "onLeave">) {
  return (
    <div className="soc__messages">
      <Welcome mark="#" title="Welcome to General." text={`Welcome to ${community.name}.`} />
      {membership.joined ? (
        <div className="soc__chatdoor">
          <p>The conversation runs in the chat window, so it stays with you while you explore.</p>
          <button className="soc__primary" type="button" data-sb-linkto="Explorer/Frames/Chat"><Icon name="chat" size={16} />Open chat</button>
        </div>
      ) : (
        <div className="soc__chatdoor">
          <p>Join {community.name} to take part in the conversation.</p>
          <JoinControl community={community} membership={membership} onJoin={onJoin} onLeave={onLeave} />
        </div>
      )}
    </div>
  );
}

type CommunitiesProps = {
  communities?: SocialCommunity[] | null;
  isLoading?: boolean;
  isError?: boolean;
  onRetry?: () => void;
  friends?: readonly SocialFriend[];
  friendsLoading?: boolean;
  isGuest?: boolean;
  events?: SocialEvent[];
  eventsLoading?: boolean;
  nearby?: SocialPerson[];
  selfAddress?: string | null;
  detail?: SocialDetail | null;
  membership?: SocialMembership;
  onSelect?: (id: string | null) => void;
  onJoin?: (community: SocialCommunity) => void;
  onLeave?: (community: SocialCommunity) => void;
  onAddFriend?: (address: string) => void;
  onMessage?: (address: string) => void;
  onCopyLink?: (id: string) => void;
  onCreate?: () => void;
  initialSection?: SectionId;
  initialCommunityId?: string | null;
  initialChannel?: ChannelId;
  now?: number;
};

const NO_COMMUNITIES: SocialCommunity[] = [];
const NO_FRIENDS: readonly SocialFriend[] = [];
const NO_EVENTS: SocialEvent[] = [];
const NO_PEOPLE: SocialPerson[] = [];

export default function Communities({
  communities,
  isLoading = false,
  isError = false,
  onRetry,
  friends = NO_FRIENDS,
  friendsLoading = false,
  isGuest = false,
  events = NO_EVENTS,
  eventsLoading = false,
  nearby = NO_PEOPLE,
  selfAddress,
  detail = null,
  membership,
  onSelect,
  onJoin,
  onLeave,
  onAddFriend,
  onMessage,
  onCopyLink,
  onCreate,
  initialSection = "home",
  initialCommunityId = null,
  initialChannel = "summary",
  now,
}: CommunitiesProps) {
  const [tab, setTab] = useState<TabId>("communities");
  const [section, setSection] = useState<SectionId>(initialSection);
  const [openId, setOpenId] = useState<string | null>(initialCommunityId);
  const [channel, setChannel] = useState<ChannelId>(initialChannel);
  const [query, setQuery] = useState("");
  const [sent, setSent] = useState<ReadonlySet<string>>(new Set());

  const all = communities ?? NO_COMMUNITIES;
  const mine = useMemo(() => all.filter(isMember), [all]);
  const current = openId && detail?.id === openId ? detail : null;
  const community = current?.community ?? all.find((c) => c.id === openId) ?? null;
  const state: SocialMembership = membership ?? { joined: Boolean(community && isMember(community)) };

  const goSection = (id: SectionId) => {
    setSection(id);
    setOpenId(null);
    onSelect?.(null);
  };
  const open = (id: string) => {
    setOpenId(id);
    setChannel("summary");
    onSelect?.(id);
  };
  const addFriend = (address: string) => {
    onAddFriend?.(address);
    setSent((prev) => new Set(prev).add(address.toLowerCase()));
  };

  const active = SECTIONS.find((s) => s.id === section) ?? SECTIONS[0]!;
  const channelMeta = CHANNELS.find((c) => c.id === channel);
  const pending: SocialDetail = current ?? { id: openId ?? "", community, members: [], posts: [], places: [], isPending: true, postsPending: true, placesPending: true };

  let body;
  if (openId) {
    if (!community) {
      body = current?.isError || (current && !current.isPending)
        ? <div className="soc__page"><ContentStatus message="Couldn't load this community." /></div>
        : <div className="soc__page"><ContentStatus pending message="Opening community&hellip;" /></div>;
    } else if (channel === "general") {
      body = <General community={community} membership={state} onJoin={onJoin} onLeave={onLeave} />;
    } else if (channel === "announcements") {
      body = <Announcements detail={pending} now={now} />;
    } else if (channel === "hangouts") {
      body = <Hangouts detail={pending} name={community.name} now={now} />;
    } else if (channel === "members") {
      body = <Members detail={pending} name={community.name} friends={friends} selfAddress={selfAddress} sent={sent} onAddFriend={addFriend} />;
    } else {
      body = <Summary detail={pending} community={community} membership={state} onJoin={onJoin} onLeave={onLeave} onCopyLink={onCopyLink} onChannel={setChannel} now={now} />;
    }
  } else if (section === "explore") {
    body = <Explore communities={all} mine={mine} query={query} isLoading={isLoading} isError={isError} onRetry={onRetry} onOpen={open} />;
  } else if (section === "friends") {
    body = <FriendsView friends={friends} loading={friendsLoading} isGuest={isGuest} onMessage={onMessage} />;
  } else if (section === "nearby") {
    body = <NearbyView nearby={nearby} friends={friends} selfAddress={selfAddress} sent={sent} onAddFriend={addFriend} />;
  } else {
    body = (
      <Home
        friends={friends}
        friendsLoading={friendsLoading}
        isGuest={isGuest}
        communities={all}
        mine={mine}
        communitiesLoading={isLoading}
        communitiesError={isError}
        onRetry={onRetry}
        events={events}
        eventsLoading={eventsLoading}
        onSection={goSection}
        onOpen={open}
        onMessage={onMessage}
        onCreate={onCreate}
      />
    );
  }

  return (
    <ExploreChrome active={tab} onTab={setTab}>
      <div className="soc">
        <nav className="soc__rail" aria-label="Sections and communities">
          {SECTIONS.map((s) => {
            const on = !openId && s.id === section;
            return (
              <button key={s.id} type="button" className={on ? "is-active" : ""} aria-label={s.label} title={s.label} aria-current={on ? "page" : undefined} onClick={() => goSection(s.id)}>
                <Icon name={s.icon} />
              </button>
            );
          })}
          {mine.length > 0 && <span className="soc__raildivider" aria-hidden="true" />}
          {mine.map((c) => (
            <button key={c.id} type="button" className={openId === c.id ? "is-active" : ""} aria-label={c.name} title={c.name} aria-current={openId === c.id ? "page" : undefined} onClick={() => open(c.id)}>
              <Cover community={c} className="soc__railthumb" />
            </button>
          ))}
          <button type="button" aria-label="Create a community" title="Create a community" onClick={onCreate} data-sb-linkto={onCreate ? undefined : "Explorer/Components/CommunityCreate"}>
            <Icon name="plus" />
          </button>
        </nav>

        <aside className="soc__side" aria-label={community && openId ? community.name : "Your space"}>
          <div className="soc__sidetitle">
            {openId ? (
              <button type="button" className="soc__sidehome" aria-current={channel === "summary" ? "page" : undefined} onClick={() => setChannel("summary")}>
                <strong className="u-truncate">{community?.name ?? "Community"}</strong>
              </button>
            ) : <strong>Your space</strong>}
          </div>
          {openId ? (
            <>
              <div className="soc__sidelabel">Channels</div>
              {CHANNELS.map((c) => (
                <button key={c.id} type="button" className={"soc__channel" + (channel === c.id ? " is-active" : "")} aria-current={channel === c.id ? "page" : undefined} onClick={() => setChannel(c.id)}>
                  <Icon name={c.icon} size={18} />
                  {c.label}
                  {c.id === "members" && community ? <small>{fmt(community.membersCount)}</small> : null}
                </button>
              ))}
            </>
          ) : (
            <>
              {SECTIONS.map((s) => (
                <button key={s.id} type="button" className={"soc__channel" + (s.id === section ? " is-active" : "")} aria-current={s.id === section ? "page" : undefined} onClick={() => goSection(s.id)}>
                  <Icon name={s.icon} size={18} />
                  {s.label}
                </button>
              ))}
              <button type="button" className="soc__channel" data-sb-linkto="Explorer/Components/Notifications">
                <Icon name="bell" size={18} />
                Invitations &amp; requests
              </button>
              {mine.length > 0 && (
                <>
                  <div className="soc__sidelabel">Your communities</div>
                  {mine.map((c) => (
                    <button key={c.id} type="button" className="soc__channel" onClick={() => open(c.id)}>
                      <Cover community={c} className="soc__sidethumb" />
                      <span className="u-truncate">{c.name}</span>
                    </button>
                  ))}
                </>
              )}
            </>
          )}
        </aside>

        <section className="soc__main" aria-label={openId ? (channelMeta?.label ?? community?.name ?? "Community") : active.label}>
          <header className="soc__mainhead">
            <span className="soc__hash" aria-hidden="true"><Icon name={openId ? (channelMeta?.icon ?? "home") : active.icon} size={22} /></span>
            <div>
              <strong>{openId ? (channelMeta?.label ?? "Overview") : active.label}</strong>
              {openId && community ? <small className="u-truncate">{community.name}</small> : null}
            </div>
            {!openId && section === "explore" && (
              <input className="soc__search" type="search" aria-label="Find a community" placeholder="Find a community" value={query} onChange={(e) => setQuery(e.target.value)} />
            )}
          </header>
          <div className="soc__body">{body}</div>
        </section>
      </div>
    </ExploreChrome>
  );
}
