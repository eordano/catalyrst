import type { QueryClient } from "@tanstack/react-query";
import type { CSSProperties, MouseEvent as ReactMouseEvent } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";

import { Avatar, fmt, hueFromSeed } from "../../atoms/primitives";
import SearchField from "../../atoms/SearchField";
import UpcomingEventCard from "../../web/components/UpcomingEventCard";
import CommunityCreate from "../../explorer/components/CommunityCreate";
import ContextMenu from "../../components/ContextMenu";
import type { ContextMenuItem } from "../../components/ContextMenu";
import { siteUrl } from "../../data/site";
import { qk, STALE } from "../../data/queryKeys";
import { loadCommunities } from "../../data/catalyst/communitiesSchema";
import type { Community, CommunityMember } from "../../data/catalyst/communities";
import {
  useCommunities,
  useCommunity,
  useCommunityPosts,
  useCommunityPlaces,
  useJoinCommunity,
  useLeaveCommunity,
} from "../../data/hooks/useCommunities";
import { useFriends } from "../../data/hooks/useFriends";
import { requestFriendAction } from "../../data/hooks/friendActions";

import "../../explorer/pages/communities.css";
import "../../explorer/pages/communitymembers.css";

type VarStyle = CSSProperties & { [k: `--${string}`]: string | number };
const cssVars = (s: VarStyle): CSSProperties => s;

type MaybeHttpError = { status?: number; message?: string } | null | undefined;

export function prefetch(queryClient: QueryClient) {
  try {
    queryClient.prefetchQuery({
      queryKey: qk.communities({}),
      queryFn: ({ signal }) => loadCommunities({}, { signal }),
      staleTime: STALE.communities,
    });
  } catch {
  }
}

const isHttpUrl = (s: unknown): boolean => typeof s === "string" && /^https?:\/\//i.test(s);

function monogram(name: unknown): string {
  const parts = String(name || "").trim().split(/\s+/).filter(Boolean);
  const first = parts[0];
  if (!first) return "?";
  if (parts.length === 1) return first.slice(0, 2).toUpperCase();
  const second = parts[1];
  return ((first[0] ?? "") + (second?.[0] ?? "")).toUpperCase();
}

function bannerStyle(c: Community): VarStyle {
  const hue = hueFromSeed(c.id || c.name || "");
  if (isHttpUrl(c.thumbnailUrl)) {
    return { "--hue": hue, backgroundImage: `url("${c.thumbnailUrl}")`, backgroundSize: "cover", backgroundPosition: "center" };
  }
  return { "--hue": hue };
}

function visLabel(c: Community): string {
  if (c.visibility === "unlisted") return "Unlisted";
  return c.privacy === "private" ? "Private" : "Public";
}

const BULLETS = [
  { icon: "\u{2665}", text: "Connect over shared interests" },
  { icon: "\u{2691}", text: "Get notified about community events" },
  { icon: "\u{2726}", text: "Chat in a shared channel" },
];

type SidebarProps = {
  onCreate: () => void;
  mine: Community[];
  showingMine: boolean;
  onToggleMine: () => void;
  onOpen: (id: string) => void;
};

function Sidebar({ onCreate, mine, showingMine, onToggleMine, onOpen }: SidebarProps) {
  return (
    <aside className="cm__side">
      <button className="cm__create" type="button" onClick={onCreate}>
        + Create a Community
      </button>

      <button className="cm__invites" type="button" data-sb-linkto="Explorer/Components/Notifications">
        <span>Invites &amp; Requests</span>
      </button>

      <div className="cm__minesection">
        <div className="cm__minehead">
          <span className="cm__minelabel">My Communities</span>
          {mine.length > 0 && (
            <button className="cm__mineviewall" type="button" onClick={onToggleMine}>
              {showingMine ? "\u{2039} BACK" : "VIEW ALL \u{203A}"}
            </button>
          )}
        </div>
        {mine.length === 0 ? (
          <span className="cm__mineempty">You haven&#x2019;t joined any communities yet.</span>
        ) : (
          mine.map((c) => (
            <div
              className="cm__minerow"
              key={c.id}
              role="button"
              tabIndex={0}
              onClick={() => onOpen(c.id)}
              onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onOpen(c.id); } }}
            >
              <Avatar
                size={32}
                name={c.name}
                seed={c.id || c.name}
                src={isHttpUrl(c.thumbnailUrl) ? c.thumbnailUrl ?? undefined : undefined}
              />
              <span className="cm__minename u-truncate" title={c.name}>{c.name}</span>
            </div>
          ))
        )}
      </div>

      <div className="cm__promo">
        <div className="cm__promotitle">
          <span className="cm__promospark" aria-hidden="true">&#x2726;</span>
          Discover Communities!
        </div>
        <div className="cm__promosub">Find your people. Join the fun.</div>

        <ul className="cm__bullets">
          {BULLETS.map((b) => (
            <li className="cm__bullet" key={b.text}>
              <span className="cm__bicon" aria-hidden="true">{b.icon}</span>
              {b.text}
            </li>
          ))}
        </ul>
      </div>
    </aside>
  );
}

function joinErrorText(err: MaybeHttpError): string {
  const status = err?.status ?? 0;
  if (status === 401 || status === 403 || status === 501) {
    return "Joining isn\u{2019}t available on this realm yet";
  }
  if (status === 0) return "Couldn\u{2019}t reach the server \u{2014} try again";
  return err?.message || "Couldn\u{2019}t join \u{2014} please try again";
}

const ERR_STYLE: CSSProperties = {
  display: "block",
  marginTop: 6,
  fontSize: 11,
  lineHeight: 1.3,
  color: "#ff8aa0",
  fontWeight: 600,
};

type JoinButtonProps = {
  id: string;
  privacy?: string;
  joined?: unknown;
  variant: "card" | "detail";
};

function JoinButton({ id, privacy, joined, variant }: JoinButtonProps) {
  const join = useJoinCommunity();
  const leave = useLeaveCommunity();
  const [override, setOverride] = useState<"joined" | "left" | "requested" | null>(null);
  const isPublic = privacy !== "private";
  const pending = join.isPending || leave.isPending;

  const effectiveJoined =
    override === "joined" ? true : override === "left" ? false : Boolean(joined);
  const requested = override === "requested";
  const err = join.error || leave.error;

  const doJoin = (e?: ReactMouseEvent) => {
    e?.stopPropagation?.();
    if (pending) return;
    setOverride(null);
    leave.reset();
    join.mutate(
      { id, privacy },
      { onSuccess: () => setOverride(isPublic ? "joined" : "requested") },
    );
  };
  const doLeave = (e?: ReactMouseEvent) => {
    e?.stopPropagation?.();
    if (pending) return;
    setOverride(null);
    join.reset();
    leave.mutate({ id }, { onSuccess: () => setOverride("left") });
  };

  if (variant === "detail") {
    let label;
    if (effectiveJoined) label = leave.isPending ? "LEAVING\u{2026}" : "LEAVE";
    else if (requested) label = "REQUESTED";
    else if (join.isPending) label = isPublic ? "JOINING\u{2026}" : "REQUESTING\u{2026}";
    else label = isPublic ? "JOIN" : "REQUEST TO JOIN";
    return (
      <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end" }}>
        <button
          className="cmb__join"
          type="button"
          disabled={pending || requested}
          onClick={effectiveJoined ? doLeave : doJoin}
        >
          {label}
        </button>
        {err ? <span style={ERR_STYLE} role="alert">{joinErrorText(err)}</span> : null}
      </div>
    );
  }

  let cardEl;
  if (effectiveJoined) {
    cardEl = (
      <button className="cm__joined" type="button" disabled={pending} onClick={doLeave}>
        {leave.isPending ? "Leaving\u{2026}" : "Joined"}
        <span className="cm__btncoin" aria-hidden="true">&#x25C6;</span>
      </button>
    );
  } else if (requested) {
    cardEl = (
      <button className="cm__join" type="button" disabled>
        Requested
        <span className="cm__btncoin" aria-hidden="true">&#x25C6;</span>
      </button>
    );
  } else {
    let label;
    if (join.isPending) label = isPublic ? "Joining\u{2026}" : "Requesting\u{2026}";
    else label = isPublic ? "Join" : "Request to join";
    cardEl = (
      <button className="cm__join" type="button" disabled={pending} onClick={doJoin}>
        {label}
        <span className="cm__btncoin" aria-hidden="true">&#x25C6;</span>
      </button>
    );
  }
  return (
    <>
      {cardEl}
      {err ? <span style={ERR_STYLE} role="alert">{joinErrorText(err)}</span> : null}
    </>
  );
}

type CommunityCardProps = { c: Community; onOpen: (id: string) => void };

function CommunityCard({ c, onOpen }: CommunityCardProps) {
  const isPublic = c.privacy !== "private";
  const joined = Boolean(c.role) && c.role !== "none";
  return (
    <article className="cm__card" onClick={() => onOpen(c.id)} role="button" tabIndex={0}
      onKeyDown={(e) => { if (e.target !== e.currentTarget) return; if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onOpen(c.id); } }}
      title={c.name}>
      <div className="cm__banner" style={bannerStyle(c)}>
        {c.isLive ? (
          <span style={{ position: "absolute", top: 10, left: 10, padding: "3px 8px", borderRadius: 999, background: "rgba(0,0,0,.55)", color: "#ff5a7a", fontSize: 11, fontWeight: 800, letterSpacing: ".04em" }}>&#x25CF; LIVE</span>
        ) : null}
        {isHttpUrl(c.thumbnailUrl) ? null : (
          <span className="cm__bannermark" aria-hidden="true">{monogram(c.name)}</span>
        )}
      </div>
      <div className="cm__cardbody">
        <div className="cm__name u-truncate" title={c.name}>{c.name}</div>
        <div className="cm__meta">
          <span className="cm__vispill">
            <span className="cm__visicon" aria-hidden="true">{isPublic ? "\u{1F310}" : "\u{1F512}"}</span>
            <span className={"cm__vis" + (isPublic ? " is-public" : "")}>{visLabel(c)}</span>
          </span>
          <span className="cm__memberchip">
            <span className="cm__membicon" aria-hidden="true">&#x1F464;</span>
            {fmt(c.membersCount)} Members
          </span>
        </div>
        <div className="cm__actions">
          <JoinButton id={c.id} privacy={c.privacy} joined={joined} variant="card" />
        </div>
      </div>
    </article>
  );
}

function isMember(c: Community): boolean {
  return Boolean(c.role) && c.role !== "none";
}

function CommunityList({ onOpen }: { onOpen: (id: string) => void }) {
  const [query, setQuery] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [tab, setTab] = useState<"browse" | "mine">("browse");
  const { data, isLoading, isError, error, refetch } = useCommunities();

  const all: Community[] = data ?? [];
  const matches = (c: Community, q: string) =>
    !q || (c.name || "").toLowerCase().includes(q) || (c.ownerName || "").toLowerCase().includes(q);
  const q = query.trim().toLowerCase();
  const mine = useMemo(() => all.filter(isMember), [all]);
  const filtered = useMemo(() => all.filter((c) => matches(c, q)), [all, q]);
  const mineFiltered = useMemo(() => mine.filter((c) => matches(c, q)), [mine, q]);
  const showingMine = tab === "mine";
  const gridList = showingMine ? mineFiltered : filtered;

  let body;
  if (isLoading) {
    body = (
      <div className="cm__grid" aria-busy="true">
        {Array.from({ length: 10 }).map((_, i) => (
          <article className="cm__card" key={i}>
            <div className="cm__banner" style={cssVars({ "--hue": (i * 36) % 360, opacity: 0.5 })} />
            <div className="cm__cardbody"><div className="cm__name">Loading&#x2026;</div></div>
          </article>
        ))}
      </div>
    );
  } else if (isError) {
    body = (
      <div className="cm__seclabel" role="alert">
        Couldn&#x2019;t load communities{error?.message ? ` \u{2014} ${error.message}` : ""}.
      </div>
    );
  } else if (gridList.length === 0) {
    body = (
      <div className="cm__seclabel">
        {showingMine
          ? "You haven\u{2019}t joined any communities yet."
          : query
            ? "No communities match your search."
            : "No communities found."}
      </div>
    );
  } else {
    body = (
      <div className="cm__grid">
        {gridList.map((c) => <CommunityCard key={c.id} c={c} onOpen={onOpen} />)}
      </div>
    );
  }

  return (
    <div className="cm">
      <Sidebar
        onCreate={() => setShowCreate(true)}
        mine={mine}
        showingMine={showingMine}
        onToggleMine={() => setTab((t) => (t === "mine" ? "browse" : "mine"))}
        onOpen={onOpen}
      />
      {showCreate && (
        <CommunityCreate
          onDone={(created) => {
            setShowCreate(false);
            if (created) void refetch();
          }}
        />
      )}
      <section className="cm__main">
        <div className="cm__header">
          <h1 className="cm__title">Communities</h1>
          <div className="cm__headactions">
            <button className="cm__createtop" type="button" onClick={() => setShowCreate(true)}>
              + Create a Community
            </button>
            <div className="cm__searchwrap">
              <SearchField placeholder="Search" value={query} onChange={setQuery} />
            </div>
          </div>
        </div>
        <div className="cm__section">
          <div className="cm__seclabel">
            {showingMine ? "My Communities" : "Browse Communities"} {isLoading ? "" : `(${gridList.length})`}
          </div>
          {body}
        </div>
      </section>
    </div>
  );
}

const ROLE_CLASS: Record<string, string> = { owner: "is-owner", moderator: "is-mod" };
const cap = (s: string): string => (s ? s.charAt(0).toUpperCase() + s.slice(1) : "");

const DETAIL_TABS = [
  { id: "announcements", label: "Announcements" },
  { id: "members", label: "Members" },
  { id: "places", label: "Places" },
  { id: "photos", label: "Photos" },
];

function GlobeIcon() {
  return (
    <svg className="cmb__globe" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2">
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" />
    </svg>
  );
}

function ChatIcon() {
  return (
    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" aria-hidden="true">
      <path d="M20 2H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h4l4 4 4-4h4c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
    </svg>
  );
}

function CopyIcon() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" aria-hidden="true">
      <rect x="9" y="9" width="11" height="11" rx="2" stroke="currentColor" strokeWidth="1.7" />
      <path d="M5 15V5a2 2 0 0 1 2-2h8" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  );
}

function AddFriendIcon() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
      <path d="M12 4a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM4 20a8 8 0 0 1 16 0z" />
    </svg>
  );
}

function Verified() {
  return (
    <svg className="cmb__verified" viewBox="0 0 24 24" width="13" height="13" aria-label="verified">
      <path d="M12 2l2.4 1.8 3-.3 1 2.8 2.6 1.5-.9 2.9.9 2.9-2.6 1.5-1 2.8-3-.3L12 22l-2.4-1.8-3 .3-1-2.8L3 16.2l.9-2.9L3 10.4l2.6-1.5 1-2.8 3 .3L12 2z" fill="#4d8dff" />
      <path d="M8.5 12l2.2 2.2 4.5-4.5" stroke="#fff" strokeWidth="1.8" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function postDate(iso?: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

function shortAddress(addr?: string | null): string {
  return `${(addr || "").slice(0, 6)}\u{2026}${(addr || "").slice(-4)}`;
}

function PostsTab({ id }: { id: string }) {
  const { data, isLoading } = useCommunityPosts(id);
  const posts = data ?? [];
  if (isLoading) return <div className="cmb__empty" aria-busy="true">Loading announcements&#x2026;</div>;
  if (posts.length === 0) return <div className="cmb__empty">No announcements yet.</div>;
  return (
    <ul className="cmb__posts">
      {posts.map((p) => (
        <li className="cmb__post" key={p.id}>
          <Avatar
            size={36}
            name={p.authorName}
            seed={p.authorAddress}
            src={isHttpUrl(p.authorProfilePictureUrl) ? p.authorProfilePictureUrl : undefined}
          />
          <div className="cmb__postbody">
            <div className="cmb__posthead">
              <span className="cmb__postauthor">{p.authorName || shortAddress(p.authorAddress)}</span>
              {p.authorHasClaimedName && <Verified />}
              <span className="cmb__postdate">&#xB7; {postDate(p.createdAt)}</span>
              <span className="cmb__postlikes">&#x2665; {p.likesCount}</span>
            </div>
            <p className="cmb__posttext">{p.content}</p>
          </div>
        </li>
      ))}
    </ul>
  );
}

function PlacesTab({ id }: { id: string }) {
  const { data, isLoading } = useCommunityPlaces(id);
  const places = data ?? [];
  if (isLoading) return <div className="cmb__empty" aria-busy="true">Loading places&#x2026;</div>;
  if (places.length === 0) return <div className="cmb__empty">No places shared yet.</div>;
  return (
    <div className="cmb__placegrid">
      {places.map((p) => (
        <div className="cmb__place" key={p.id}>
          <div className="cmb__placecover" style={cssVars({ "--hue": hueFromSeed(p.id) })} />
          <span className="cmb__placename u-truncate" title={p.id}>{p.id}</span>
          <span className="cmb__placemeta">Added by {shortAddress(p.addedBy)} &#xB7; {postDate(p.addedAt)}</span>
        </div>
      ))}
    </div>
  );
}

type CommunityMenuProps = { id: string; joined?: boolean; onLeft: () => void };

function CommunityMenu({ id, joined, onLeft }: CommunityMenuProps) {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const leave = useLeaveCommunity();
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e: PointerEvent) => {
      if (wrapRef.current && e.target instanceof Node && !wrapRef.current.contains(e.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open]);

  const items: ContextMenuItem[] = [
    {
      kind: "button",
      label: copied ? "Copied!" : "Copy link",
      icon: <CopyIcon />,
      onClick: () => {
        void navigator.clipboard?.writeText(siteUrl(`/communities/${id}`)).catch(() => undefined);
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      },
    },
  ];
  if (joined) {
    items.push({ kind: "separator" });
    items.push({
      kind: "button",
      label: leave.isPending ? "Leaving\u{2026}" : "Leave community",
      danger: true,
      disabled: leave.isPending,
      onClick: () => {
        setOpen(false);
        leave.mutate({ id }, { onSuccess: onLeft });
      },
    });
  }

  return (
    <div className="cmb__kebabwrap" ref={wrapRef}>
      <button
        type="button"
        className="cmb__kebab"
        aria-label="More options"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        &#x22EE;
      </button>
      {open && (
        <div className="cmb__menuanchor">
          <ContextMenu items={items} onClose={() => setOpen(false)} autoFocus />
        </div>
      )}
    </div>
  );
}

function CommunityDetail({ id, onClose }: { id: string; onClose: () => void }) {
  const [tab, setTab] = useState("members");
  const [memberQuery, setMemberQuery] = useState("");
  const [justSent, setJustSent] = useState<ReadonlySet<string>>(new Set());
  const { data, isLoading, isError, error } = useCommunity(id);
  const { friends } = useFriends();
  const navigate = useNavigate();

  const community: Community | null = data?.community ?? null;
  const members = data?.members ?? [];
  const events = data?.events ?? [];

  const friendSet = useMemo(
    () => new Set(friends.map((f) => f.address.toLowerCase())),
    [friends],
  );

  const filteredMembers = useMemo(() => {
    const q = memberQuery.trim().toLowerCase();
    if (!q) return members;
    return members.filter((m) => (m.name || "").toLowerCase().includes(q) || (m.memberAddress || "").toLowerCase().includes(q));
  }, [members, memberQuery]);

  const addFriend = (m: CommunityMember) => {
    if (!m.memberAddress) return;
    requestFriendAction("request", m.memberAddress);
    setJustSent((prev) => new Set(prev).add(m.memberAddress.toLowerCase()));
  };

  const hue = hueFromSeed(id);
  const joined = Boolean(community && community.role && community.role !== "none");
  const thumbStyle: CSSProperties = community && isHttpUrl(community.thumbnailUrl)
    ? { backgroundImage: `url("${community.thumbnailUrl}")`, backgroundSize: "cover", backgroundPosition: "center" }
    : { background: `linear-gradient(135deg, hsl(${hue} 70% 60%), hsl(${(hue + 40) % 360} 60% 38%))` };

  return (
    <div className="cmb__backdrop" onClick={onClose}>
      <div className="cmb" onClick={(e) => e.stopPropagation()}>
        <button className="cmb__close" aria-label="Close" type="button" onClick={onClose}>&#xD7;</button>

        <div className="cmb__main">
          <header className="cmb__header">
            <span className="cmb__thumb" style={thumbStyle} />
            <div className="cmb__headinfo">
              <h2 className="cmb__cname u-truncate">{community?.name || (isLoading ? "Loading\u{2026}" : "Community")}</h2>
              <div className="cmb__meta">
                <GlobeIcon />
                {community ? `${visLabel(community)} \u{B7} ${fmt(community.membersCount)} Members` : "\u{2014}"}
              </div>
              <p className="cmb__about">
                {community?.description
                  || (isError ? `Couldn\u{2019}t load this community${error?.message ? ` \u{2014} ${error.message}` : ""}.` : "")}
              </p>
            </div>
            {community ? (
              <div className="cmb__headactions">
                <JoinButton id={id} privacy={community.privacy} joined={joined} variant="detail" />
                {joined && (
                  <button
                    type="button"
                    className="cmb__chatbtn"
                    aria-label="Open chat"
                    title="Open chat"
                    onClick={() => navigate("/chat")}
                  >
                    <ChatIcon />
                  </button>
                )}
                <CommunityMenu id={id} joined={joined} onLeft={onClose} />
              </div>
            ) : null}
          </header>

          <nav className="cmb__tabs" role="tablist">
            {DETAIL_TABS.map((t) => (
              <button
                key={t.id}
                role="tab"
                type="button"
                aria-selected={t.id === tab}
                className={"cmb__tab" + (t.id === tab ? " is-active" : "")}
                onClick={() => setTab(t.id)}
              >
                {t.label}
              </button>
            ))}
          </nav>

          <div className="cmb__body">
            {tab === "announcements" ? (
              <PostsTab id={id} />
            ) : tab === "places" ? (
              <PlacesTab id={id} />
            ) : tab === "photos" ? (
              <div className="cmb__empty">No photos shared yet.</div>
            ) : isLoading ? (
              <div className="cmb__empty" aria-busy="true">Loading members&#x2026;</div>
            ) : filteredMembers.length === 0 ? (
              <div className="cmb__empty">{memberQuery ? "No members match your search." : "No members to show."}</div>
            ) : (
              <ul className="cmb__grid">
                {filteredMembers.map((m) => {
                  const role = (m.role || "member").toLowerCase();
                  const addrLc = (m.memberAddress || "").toLowerCase();
                  const isFriend = addrLc !== "" && friendSet.has(addrLc);
                  const requested = justSent.has(addrLc);
                  return (
                    <li className="cmb__row" key={m.memberAddress || m.name}>
                      <Avatar
                        size={44}
                        name={m.name}
                        seed={m.memberAddress || m.name}
                        src={isHttpUrl(m.profilePictureUrl) ? m.profilePictureUrl : undefined}
                      />
                      <div className="cmb__info">
                        <div className="cmb__name">
                          <span className="u-truncate">{m.name || shortAddress(m.memberAddress)}</span>
                          {m.hasClaimedName && <Verified />}
                        </div>
                        {role !== "member" && (
                          <span className={"cmb__role " + (ROLE_CLASS[role] || "")}>{cap(role)}</span>
                        )}
                      </div>
                      {isFriend ? (
                        <span className="cmb__add is-friend">Already Friend</span>
                      ) : requested ? (
                        <span className="cmb__add is-sent">Requested</span>
                      ) : (
                        <button className="cmb__add" type="button" onClick={() => addFriend(m)}>
                          <AddFriendIcon />
                          ADD FRIEND
                        </button>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </div>

          {tab === "members" && (
          <form className="cmb__search" onSubmit={(e) => e.preventDefault()}>
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" />
            </svg>
            <input
              type="text"
              placeholder="Search member or name"
              aria-label="Search members"
              value={memberQuery}
              onChange={(e) => setMemberQuery(e.target.value)}
            />
            <button className="cmb__searchgo" type="submit" aria-label="Search">
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2.5">
                <path d="M5 12h14M13 6l6 6-6 6" />
              </svg>
            </button>
          </form>
          )}
        </div>

        <aside className="cmb__events">
          <h3 className="cmb__eventstitle">Upcoming Events</h3>
          <div className="cmb__eventlist">
            {events.length === 0 ? (
              <div className="cmb__empty" style={{ height: 120 }}>No upcoming events.</div>
            ) : (
              events.map((e) => (
                <UpcomingEventCard
                  key={e.id}
                  event={{ title: e.name ?? "Untitled event", when: e.timeLabel ?? "\u{2014}", image: isHttpUrl(e.image) ? (e.image ?? undefined) : undefined, hue: hueFromSeed(e.id) }}
                />
              ))
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}

export default function CommunitiesPanel() {
  const [selectedId, setSelectedId] = useState<string | null>(null);

  return (
    <>
      <CommunityList onOpen={setSelectedId} />
      {selectedId ? (
        <CommunityDetail id={selectedId} onClose={() => setSelectedId(null)} />
      ) : null}
    </>
  );
}
