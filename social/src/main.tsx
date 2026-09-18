import { warmPublicDiscovery } from "./public-feeds";
import type { FriendsState } from "./friends";
import { destinationLabel } from "./destinations";
import { LiveCommunities } from "./LiveCommunities";
import { CommunityHangouts } from "./CommunityHangouts";
import { Home } from "./Home";
import { EventsPage, EventActivity } from "./Events";
import { EventEditor } from "./EventEditor";
import { WorldsPage } from "./Worlds";
import { CommunityInbox, PendingCommunityRequest } from "./CommunityInbox";
import { ChannelSettings } from "./ChannelSettings";
import { Invite } from "./Invite";
import React, { lazy, Suspense, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  api,
  ApiError,
  execute,
  existingIdentity,
  shortWallet,
  basePath,
  type Community,
  type Message,
  type Opened,
  type Post,
  type Scene,
  type WalletIdentity,
} from "./api";
import "./style.css";
import { Avatar, Name } from "./Profile";
import { forgetWalletSession } from "./session";
import { NotificationPreference, ActivityInbox, useActivity, FriendRequestActivity } from "./notifications";
import { JoinRequests } from "./JoinRequests";
import { PlaceCard } from "./PlaceCard";
import { CommunityEditor } from "./CommunityEditor";
import { Icon } from "./Icon";
import { Dialog } from "./Dialog";
import { ProfileDialog } from "./ProfileDialog";
import { ConversationPanel, type Panel } from "./ConversationPanel";
import { ScenePicker } from "./ScenePicker";

import { Picture, communityImage } from "./Picture";
import { installTelemetry } from "./telemetry";
installTelemetry();
const Friends = lazy(() => import("./Friends").then(m => ({ default: m.Friends })));

function readRoute() {
  const hash = window.location.hash;
  return /^(#\/(home|discover|mine|friends|nearby|events|worlds|invitations|notifications)|#\/events\/(?:[0-9a-f-]{36}|rsvps)|#\/friends\/(requests|blocked)|#\/dm\/0x[0-9a-f]{40}|#\/c\/[0-9a-f-]{36}\/([a-z0-9-]{1,32}))$/i.test(
    hash,
  )
    ? hash
    : "#/home";
}
function App() {
  useEffect(() => {
    const timer = setTimeout(() => { warmPublicDiscovery(); void import("./Friends"); }, 0);
    return () => clearTimeout(timer);
  }, []);
  const [online,setOnline] = useState(navigator.onLine);
  useEffect(() => { const update = () => setOnline(navigator.onLine); window.addEventListener("online",update); window.addEventListener("offline",update); return () => { window.removeEventListener("online",update); window.removeEventListener("offline",update); }; },[]);
  const [createAfterConnect,setCreateAfterConnect] = useState(false);
  const [editing, setEditing] = useState<"create" | "settings" | null>(null);
  const [panel, setPanel] = useState<Panel | null>(null);
  const [threadMessage,setThreadMessage]=useState<Message>();
  const [historyMore,setHistoryMore]=useState(false),[olderBusy,setOlderBusy]=useState(false);
  const [thread, setThread] = useState("");
  const [profile, setProfile] = useState("");
  const [communityMenu, setCommunityMenu] = useState(false);
  const [reported,setReported]=useState<string[]>([]);
  const [dialog, setDialog] = useState<"invite" | "preferences" | "channel" | "channelSettings" | "leave" | "requests" | "hangouts" | "event" | null>(null);
  const [channelName, setChannelName] = useState("");
  const [privateChannel, setPrivateChannel] = useState(false);
  const [channelError, setChannelError] = useState("");
  const [emoji, setEmoji] = useState(false);
  const [compact, setCompact] = useState(() => localStorage.getItem("dcl.social.compact") === "true");
  useEffect(() => { const show = (e: Event) => setProfile((e as CustomEvent<string>).detail); window.addEventListener("social:profile", show); return () => window.removeEventListener("social:profile", show); }, []);
  useEffect(() => { localStorage.setItem("dcl.social.compact", String(compact)); }, [compact]);
  const [connecting, setConnecting] = useState(false);
  const connectingRef = useRef(false);
  const [identity, setIdentity] = useState<WalletIdentity | null>(null);
  const [communities, setCommunities] = useState<Community[]>([]);
  const [preview, setPreview] = useState<Community | null>(null);
  const [joinPending, setJoinPending] = useState<Record<string, boolean>>({});
  const [opened, setOpened] = useState<Opened | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [posts, setPosts] = useState<Post[]>([]);
  const [channel, setChannel] = useState<string>(
    "general",
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [expired, setExpired] = useState(false);
  const [drafts, setDrafts] = useState<
    Record<string, { text: string; scene: Scene | null }>
  >({});
  const [picker, setPicker] = useState(false);
  const [search, setSearch] = useState("");
  const [drawer, setDrawer] = useState(false);
  const [route, setRoute] = useState(readRoute);
  const [mine, setMine] = useState<Community[]>([]);
  const [listLoading, setListLoading] = useState(false);
  const [total, setTotal] = useState(0);
  const [listError, setListError] = useState("");
  const listRequest = useRef(0);
  const mineRequest = useRef(0);
  const bottom = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLElement>(null);
  const followMessages = useRef(true);
  const navigation = useRef<HTMLElement>(null);
  const menuButton = useRef<HTMLButtonElement>(null);
  const composerInput = useRef<HTMLTextAreaElement>(null);
  const generation = useRef(0);
  const identityGeneration = useRef(0);
  const identityRef = useRef(identity);
  identityRef.current = identity;
  const openedRef = useRef(opened);
  openedRef.current = opened;
  const draftKey = `${opened?.community.id}/${channel}`;
  const draft = drafts[draftKey]?.text || "";
  const scene = drafts[draftKey]?.scene || null;
  function updateDraft(
    patch: Partial<{ text: string; scene: Scene | null }>,
    key = draftKey,
  ) {
    setDrafts((all) => ({
      ...all,
      [key]: {
        text: all[key]?.text || "",
        scene: all[key]?.scene || null,
        ...patch,
      },
    }));
  }
  const setDraft = (text: string) => updateDraft({ text });
  const setScene = (scene: Scene | null) => updateDraft({ scene });
  const [eventSearch, setEventSearch] = useState("");
  const [worldSearch, setWorldSearch] = useState("");
  const isHome = route === "#/home";
  const isEvents = route === "#/events" || route.startsWith("#/events/");
  const isNotifications = route === "#/notifications";
  const activity = useActivity(identity?.address);
  const isWorlds = route === "#/worlds";
  const isInvitations = route === "#/invitations";
  const [friendSnapshot, setFriendSnapshot] = useState<FriendsState>({friends:[],ready:false,error:""});
  const isNearby = route === "#/nearby";
  const isVoice = route.endsWith("/voice");
  const isMine = route === "#/mine";
  const isFriends = route.startsWith("#/friends") || route.startsWith("#/dm/");
  const sections = [
    {label: "Home", href: "#/home", icon: "home", active: isHome},
    {label: "Explore communities", href: "#/discover", icon: "communities", active: route === "#/discover" || isMine},
    {label: "Friends", href: "#/friends", icon: "people", active: isFriends},
    {label: "Nearby", href: "#/nearby", icon: "pin", active: isNearby},
    {label: "Events", href: "#/events", icon: "calendar", active: isEvents && route !== "#/events/rsvps"},
    {label: "Worlds", href: "#/worlds", icon: "globe", active: isWorlds},
  ] as const;
  const [friendUnread, setFriendUnread] = useState(0);
  const [friendsStarted, setFriendsStarted] = useState(false);
  useEffect(() => { if (identity || isFriends || isNearby) setFriendsStarted(true); }, [identity, isFriends, isNearby]);
  useEffect(() => { const close = (e: MouseEvent) => { if (!(e.target as Element).closest(".side-title")) setCommunityMenu(false); if (!(e.target as Element).closest(".emoji-control")) setEmoji(false); }; const escape = (e: KeyboardEvent) => { if (e.key === "Escape") {setCommunityMenu(false);setEmoji(false);} }; document.addEventListener("click",close);document.addEventListener("keydown",escape);return()=>{document.removeEventListener("click",close);document.removeEventListener("keydown",escape);}; },[]);
  const channelPath = (id: string, c = "general") => `#/c/${id}/${c}`;
  function go(next: string) {
    setDrawer(false);
    setPicker(false);
    setPanel(null);
    setCommunityMenu(false);
    if (next === route) {
      return;
    }
    window.location.hash = next;
  }
  function closeDrawer() {
    setDrawer(false);
    menuButton.current?.focus();
  }
  async function loadIdentity(connect = false) {
    if (connectingRef.current) return;
    if (connect) {
      setDrawer(false);
      connectingRef.current = true;
      setConnecting(true);
    }
    const current = ++identityGeneration.current;
    generation.current++;
    identityRef.current = null;
    openedRef.current = null;
    setIdentity(null);
    setOpened(null);
    setPreview(null);
    setMessages([]);
    setPosts([]);
    setDrafts({});
    setMine([]);
    setJoinPending({});
    setFriendUnread(0);
    setFriendSnapshot({friends:[],ready:false,error:""});
    setPanel(null);
    setEditing(null);
    setDialog(current => connect && current === "event" ? "event" : null);
    setPicker(false);
    setBusy(false);
    setError("");
    try {
      const next = await existingIdentity(connect);
      if (current === identityGeneration.current) setIdentity(next);
    } catch (e) {
      if (current === identityGeneration.current) {
        const code = (e as { code?: number })?.code;
        setError(code === 4001 ? "Connection cancelled. You can try again." :
          code === -32002 ? "A wallet request is already open. Check your wallet." :
          e instanceof Error ? e.message : "Wallet unavailable");
      }
    } finally {
      if (connect) {
        connectingRef.current = false;
        setConnecting(false);
      }
    }
  }
  useEffect(() => {
    void loadIdentity();
    const changed = () => void loadIdentity();
    const hash = () => setRoute(readRoute());
    const storage = (event: StorageEvent) => { if (event.key === "dcl.social.wallet-session.v1") changed(); };
    window.addEventListener("storage", storage);
    window.addEventListener("hashchange", hash);
    window.addEventListener("dcl:identity-changed", changed);
    window.ethereum?.on?.("accountsChanged", changed);
    return () => {
      window.removeEventListener("hashchange", hash);
      window.removeEventListener("storage", storage);
      window.removeEventListener("dcl:identity-changed", changed);
      window.ethereum?.removeListener?.("accountsChanged", changed);
    };
  }, []);
  useEffect(() => {
    if (!identity?.expiresAt) return;
    const timer = setTimeout(() => void loadIdentity(), Math.max(0, identity.expiresAt - Date.now()));
    return () => clearTimeout(timer);
  }, [identity]);
  async function loadPublic(offset = 0, signal?: AbortSignal) {
    const request = ++listRequest.current;
    setListLoading(true);
    setListError("");
    try {
      const v = await api<{ data: { results: Community[]; total: number } }>(
        `/communities?search=${encodeURIComponent(search)}&offset=${offset}`,
        { signal },
      );
      if (signal?.aborted || request !== listRequest.current) return;
      setCommunities((old) =>
        offset
          ? [
              ...old,
              ...v.data.results.filter((c) => !old.some((o) => o.id === c.id)),
            ]
          : v.data.results,
      );
      setTotal(v.data.total);
    } catch (e) {
      if (!signal?.aborted && request === listRequest.current)
        setListError(
          e instanceof Error ? e.message : "Unable to load communities",
        );
    } finally {
      if (!signal?.aborted && request === listRequest.current)
        setListLoading(false);
    }
  }
  useEffect(() => {
    const abort = new AbortController();
    const timer = setTimeout(() => void loadPublic(0, abort.signal), 200);
    return () => {
      clearTimeout(timer);
      abort.abort();
    };
  }, [search]);
  useEffect(() => {
    if (followMessages.current)
      bottom.current?.scrollIntoView({ behavior: "instant", block: "end" });
  }, [messages, channel]);
  useEffect(() => {
    const area = composerInput.current;
    if (area) {
      area.style.height = "44px";
      area.style.height = `${Math.min(area.scrollHeight, 144)}px`;
    }
  }, [draft, channel]);
  useEffect(() => {
    if (!drawer) return;
    const nav = navigation.current;
    if (!nav) return;
    nav.querySelector<HTMLElement>("button,a")?.focus();
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        closeDrawer();
      }
      if (e.key !== "Tab") return;
      const items = Array.from(
        nav.querySelectorAll<HTMLElement>(
          "button:not(:disabled),a[href],input",
        ),
      ).filter((el) => el.getClientRects().length);
      const first = items[0],
        last = items.at(-1);
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last?.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first?.focus();
      }
    };
    const resize = () => {
      if (window.innerWidth > 760) setDrawer(false);
    };
    document.addEventListener("keydown", key);
    window.addEventListener("resize", resize);
    return () => {
      document.removeEventListener("keydown", key);
      window.removeEventListener("resize", resize);
    };
  }, [drawer]);
  async function run(task: () => Promise<void>) {
    const current = generation.current;
    setBusy(true);
    setError("");
    try {
      await task();
    } catch (e) {
      if (current === generation.current)
        setError(e instanceof Error ? e.message : "Something went wrong");
    } finally {
      if (current === generation.current) setBusy(false);
    }
  }
  async function openCommunity(
    c: Community,
    target: string = "general",
  ) {
    if (!identity) {
      setError("Open your existing wallet session, then retry.");
      return;
    }
    const current = ++generation.current;
    const valid = () => current === generation.current;
    await run(async () => {
      const details = await execute<{ data: Community }>(identity, { type: "community_details", community_id: c.id }, valid);
      if (!valid()) return;
      if (!["member", "moderator", "owner"].includes(details.data.role || "")) {
        setOpened(null);
        openedRef.current = null;
        setPreview(details.data);
        if (details.data.privacy === "private") {
          const requests = await execute<{ data: { results: { communityId: string; status: string }[] } }>(identity, { type: "my_join_requests" }, valid);
          if (valid()) setJoinPending(all => ({ ...all, [c.id]: requests.data.results.some(r => r.communityId === c.id && r.status === "pending") }));
        }
        return;
      }
      setPreview(null);
      setMine(all => all.some(item=>item.id===details.data.id) ? all.map(item=>item.id===details.data.id?details.data:item) : [...all,details.data]);
      const next = await execute<Opened>(
        identity,
        { type: "open_community", community_id: c.id, channel: target === "announcements" || target === "voice" ? "general" : target },
        valid,
      );
      if (!valid()) return;
      setOpened(next);
      setMessages(next.messages);
      setHistoryMore(next.messages.length===100);
      setChannel(target);
      setExpired(false);
      followMessages.current = true;
      if (target === "announcements") {
        const result = await execute<{ data: { posts: Post[] } }>(
          identity,
          { type: "community_posts", community_id: c.id },
          valid,
        );
        if (valid()) setPosts(result.data.posts);
      }
    });
  }
  async function joinCommunity(c: Community) {
    if (!identity || busy) return;
    const current = generation.current;
    const valid = () => current === generation.current;
    await run(async () => {
      await execute(identity, { type: c.privacy === "private" ? "request_community_join" : "join_community", community_id: c.id }, valid);
      if (!valid()) return;
      if (c.privacy === "private") setJoinPending(all => ({ ...all, [c.id]: true }));
      else await openCommunity(c);
    });
  }
  async function loadMine() {
    if (!identity) return;
    const request = ++mineRequest.current;
    const valid = () => identityRef.current === identity && request === mineRequest.current;
    try {
      const result = await execute<{ data: { results: Community[] } }>(identity, { type: "my_communities" }, valid);
      if (valid()) setMine(result.data.results);
    } catch (e) {
      if (valid()) setError(e instanceof Error ? e.message : "Unable to load your communities");
    }
  }
  useEffect(() => {
    if (!identity) return;
    void loadMine();
    const refresh = () => { if (identity.canSignSilently) void loadMine(); };
    window.addEventListener("focus",refresh);
    return () => { mineRequest.current++; window.removeEventListener("focus",refresh); };
  }, [identity]);
  async function announcements() {
    if (!identity || !opened) return;
    const current = generation.current;
    await run(async () => {
      const result = await execute<{ data: { posts: Post[] } }>(
        identity,
        { type: "community_posts", community_id: opened.community.id },
        () => current === generation.current,
      );
      if (current === generation.current) setPosts(result.data.posts);
    });
  }
  useEffect(() => {
    generation.current++;
    setPanel(null);
    setCommunityMenu(false);
    setDialog(null);
    setEditing(null);
    setEmoji(false);
    setBusy(false);
    setError("");
    setPicker(false);
    setDrawer(false);
    const match = route.match(
      /^#\/c\/([0-9a-f-]{36})\/([a-z0-9-]{1,32})$/i,
    );
    if (!match) {
      setOpened(null);
      setPreview(null);
      openedRef.current = null;
      setMessages([]);
      setPosts([]);
      if (route === "#/mine") void loadMine();
      return;
    }
    const target = match[2];
    if (openedRef.current?.community.id === match[1]) {
      setChannel(target);
      if (target === "announcements") void announcements();
      else if (target !== openedRef.current.channel) void openCommunity(openedRef.current.community, target);
      return;
    }
    setOpened(null);
    setPreview(null);
    openedRef.current = null;
    setMessages([]);
    setPosts([]);
    if (identity)
      void openCommunity(
        { id: match[1], name: "Community", description: "", membersCount: 0 },
        target,
      );
  }, [route, identity]);
  useEffect(() => { if (identity && createAfterConnect) {setEditing("create");setCreateAfterConnect(false);} },[identity,createAfterConnect]);
  useEffect(() => { if (!identity && route.startsWith("#/c/")) { const c = communities.find(c => c.id === route.split("/")[2]); if (c) setPreview(c); } },[identity,communities,route]);
  function mergeMessages(all:Message[],next:Message[]){return [...new Map([...all,...next].map(m=>[m.id,m])).values()].sort((a,b)=>a.seq-b.seq);}
  async function older(){if(!opened||olderBusy||!messages.length)return;const o=opened;setOlderBusy(true);followMessages.current=false;try{const r=await api<{messages:Message[];hasMore:boolean}>(`/communities/${o.community.id}/messages?before=${messages[0].seq}`,{headers:{Authorization:`Bearer ${o.readToken}`}});if(openedRef.current?.readToken!==o.readToken)return;const height=scroller.current?.scrollHeight||0;setMessages(all=>mergeMessages(all,r.messages));setHistoryMore(r.hasMore);requestAnimationFrame(()=>{if(scroller.current)scroller.current.scrollTop+=scroller.current.scrollHeight-height;});}catch(e){setError(e instanceof Error?e.message:"Could not load older messages.");}finally{setOlderBusy(false);}}
  async function readHistory(o: Opened) {
    try {
      const result = await api<{ messages: Message[] }>(
        `/communities/${o.community.id}/messages`,
        { headers: { Authorization: `Bearer ${o.readToken}` } },
      );
      if (openedRef.current?.readToken === o.readToken)
        setMessages(all=>mergeMessages(all,result.messages));
    } catch (e) {
      if (openedRef.current?.readToken !== o.readToken) return;
      if (e instanceof ApiError && [403, 410].includes(e.status)) {
        setExpired(true);
        setMessages([]);
        setPosts([]);
        if (e.status === 403) { setOpened(null); openedRef.current = null; setPreview({ ...o.community, role: "none" }); }
      }
      setError(e instanceof Error ? e.message : "Unable to refresh");
    }
  }
  useEffect(() => {
    if (!opened || expired) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      if (stopped) return;
      if (Date.now() >= opened.expiresAt) {
        if (identityRef.current?.canSignSilently) {
          try {
            const next = await execute<Opened>(
              identityRef.current,
              { type: "open_community", community_id: opened.community.id, channel:opened.channel || "general" },
              () => !stopped,
            );
            if (!stopped) {
              setOpened(next);
              setMessages(all=>mergeMessages(all,next.messages));
            }
          } catch (e) {
            if (!stopped) {
              setExpired(true);
              setMessages([]);
              setPosts([]);
              setError(e instanceof Error ? e.message : "Unable to refresh");
            }
          }
        } else {
          setExpired(true);
          setMessages([]);
          setPosts([]);
        }
        return;
      }
      await readHistory(opened);
      if (!stopped) timer = setTimeout(poll, 4000);
    };
    timer = setTimeout(
      poll,
      Math.min(4000, Math.max(0, opened.expiresAt - Date.now())),
    );
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [opened, expired]);
  async function send(e: React.FormEvent) {
    e.preventDefault();
    if (!identity || !opened || busy || expired || (!draft.trim() && !scene))
      return;
    if (channel === "announcements" && !draft.trim()) return;
    const current = generation.current;
    const key = draftKey;
    const o = opened;
    await run(async () => {
      if (channel === "announcements") {
        const result = await execute<{ data: Post }>(
          identity,
          {
            type: "publish_post",
            community_id: o.community.id,
            content: draft,
          },
          () => current === generation.current,
        );
        updateDraft({ text: "", scene: null }, key);
        if (current === generation.current)
          setPosts((p) => [result.data, ...p]);
      } else {
        await execute(
          identity,
          {
            type: "send_message",
            channel,
            community_id: o.community.id,
            text: draft,
            scene,
          },
          () => current === generation.current,
        );
        updateDraft({ text: "", scene: null }, key);
        if (current === generation.current) {
          followMessages.current = true;
          await readHistory(o);
        }
      }
    });
  }
  async function messageAction(message: Message, action: "heart" | "celebrate" | "pin" | "unpin" | "report") {
    if (!identity || !opened || busy) return;
    const current = generation.current, o = opened;
    await run(async () => { await execute(identity, { type: "message_action", community_id: o.community.id, message_id: message.id, action }, () => current === generation.current); if (current === generation.current) { if(action==="report")setReported(ids=>[...ids,message.id]); await readHistory(o); } });
  }
  const canPublish = ["owner", "moderator"].includes(
    opened?.community.role || "",
  );
  const visible = isMine
    ? mine.filter((c) => c.name.toLowerCase().includes(search.toLowerCase()))
    : communities;
  const generalPreferences = <><NotificationPreference /><label className="preference-row"><span><strong>Compact messages</strong><small>Keep more of the conversation in view.</small></span><input type="checkbox" checked={compact} onChange={e => setCompact(e.target.checked)} /></label>{identity && <button className="outline-button" onClick={() => { setDialog(null); forgetWalletSession(); }}>Forget wallet session</button>}</>;
  const openPreferences = () => { if (identity) window.dispatchEvent(new CustomEvent("social:settings", {detail: "general"})); else setDialog("preferences"); };
  return (
    <div className={`shell ${isFriends ? "friends-mode" : ""} ${panel && opened ? "panel-open" : ""} ${compact ? "compact" : ""}`}>
      <header className="appbar">
        <a className="brand" href="#/home">
          <img src={`${basePath}assets/logo.png`} alt="" />
          decentraland
        </a>
        <span>Social</span>
        <a
          className="jump"
          href="https://decentraland.org/jump"
          target="_blank"
          rel="noreferrer"
        >
          Jump in &#x2197;
        </a>
        {identity ? <div className="account"><button aria-label="Notifications" onClick={() => go("#/notifications")}><Icon name="bell" />{activity.items.some(x=>!x.read)&&<span className="notification-dot" />}</button><button aria-label="Preferences" onClick={openPreferences}><Icon name="settings" /></button><Avatar wallet={identity.address} /></div> :
          <button className="account" disabled={connecting} onClick={() => void loadIdentity(true)}>
            {connecting ? "Connecting\u2026" : "Connect wallet"}
          </button>}
      </header>
      {drawer && (
        <button
          className="scrim"
          aria-label="Close navigation"
          onClick={closeDrawer}
        />
      )}
      <aside
        id="navigation"
        ref={navigation}
        className={`navigation ${drawer ? "is-open" : ""}`}
        role={drawer ? "dialog" : undefined}
        aria-modal={drawer || undefined}
        aria-label="Navigation"
      >
        <nav className="rail" aria-label="Sections and communities">
          {sections.map(item => <button key={item.href} aria-label={item.label} title={item.label} aria-current={item.active ? "page" : undefined} className={item.active ? "active" : ""} onClick={() => go(item.href)}><Icon name={item.icon} />{item.href === "#/friends" && friendUnread > 0 && <b className="friends-badge">{friendUnread}</b>}</button>)}
          <div className="rail-divider" />
          {mine.map((c) => (
            <button
              title={c.name}
              aria-label={c.name}
              className={opened?.community.id === c.id ? "active" : ""}
              key={c.id}
              onClick={() => go(channelPath(c.id))}
            >
              <Picture src={communityImage(c.id)} fallback={c.name.slice(0, 2)} />
            </button>
          ))}
          <button aria-label="Create a community" onClick={() => { if (identity) setEditing("create"); else {setCreateAfterConnect(true);void loadIdentity(true);} }}><Icon name="plus" /></button>
          <button className="rail-settings" aria-label="Preferences" onClick={openPreferences}><Icon name="settings" /></button>
        </nav>
        <section className="sidebar">
          <div className="side-title">
            {opened ? <button className="community-heading" aria-expanded={communityMenu} onClick={() => setCommunityMenu(v => !v)}><strong>{opened.community.name}</strong><Icon name="down" /></button> : <strong>Your space</strong>}
            {communityMenu && opened && <div className="community-menu">{canPublish && channel!=="announcements" && !isVoice && <button onClick={()=>{setDialog("channelSettings");setCommunityMenu(false);}}><Icon name="settings"/>Channel settings</button>}{canPublish&&<button onClick={()=>{setPanel("reports");setCommunityMenu(false);setDrawer(false);}}>Reported messages</button>}{canPublish&&<button onClick={()=>{setPanel("bans");setCommunityMenu(false);setDrawer(false);}}><Icon name="people"/>Banned members</button>}{canPublish && <button onClick={() => { setDialog("requests"); setCommunityMenu(false); }}><Icon name="people" />Join requests</button>}{opened.community.role === "owner" && <button onClick={() => { setEditing("settings"); setCommunityMenu(false); }}><Icon name="settings" />Community settings</button>}<button onClick={() => { setDialog("invite"); setCommunityMenu(false); }}><Icon name="plus" />Invite people</button><button onClick={() => { setPanel("members"); setCommunityMenu(false); setDrawer(false); }}><Icon name="people" />Members</button><button onClick={() => { setPanel("pins"); setCommunityMenu(false); setDrawer(false); }}><Icon name="pin" />Pinned messages</button>{opened.community.role!=="owner"&&<button onClick={()=>{setDialog("leave");setCommunityMenu(false);}}>Leave community</button>}</div>}
            <button
              className="mobile"
              aria-label="Close navigation"
              onClick={closeDrawer}
            >
              &#xd7;
            </button>
          </div>
          {opened ? (
            <>
              <div className="channel-label">Channels {canPublish && <button aria-label="Create channel" onClick={() => { setDialog("channel"); setChannelName(""); setChannelError(""); }}><Icon name="plus" /></button>}</div>
              <button
                aria-current={channel === "general" ? "page" : undefined}
                className={`channel ${channel === "general" ? "selected" : ""}`}
                onClick={() => go(channelPath(opened.community.id))}
              >
                <span>#</span> general
              </button>
              {opened.channels?.map(c => <button key={c.name} className={`channel ${channel === c.name ? "selected" : ""}`} aria-current={channel === c.name ? "page" : undefined} onClick={() => go(channelPath(opened.community.id,c.name))}><span>{c.private ? "\u2311" : "#"}</span>{c.name}</button>)}
              <button
                className={`channel ${channel === "announcements" ? "selected" : ""}`}
                aria-current={channel === "announcements" ? "page" : undefined}
                onClick={() =>
                  go(channelPath(opened.community.id, "announcements"))
                }
              >
                <Icon name="bell" /> announcements
              </button>
              <button className="channel" onClick={() => { setDialog("hangouts"); setDrawer(false); }}><Icon name="globe" />Hangouts</button>
              <div className="channel-label">Voice</div><button className={`channel ${isVoice ? "selected" : ""}`} onClick={() => go(channelPath(opened.community.id,"voice"))}><Icon name="people" />The lounge</button>
              <button className="channel" onClick={() => { setPanel("members"); setDrawer(false); }}><Icon name="people" /> Members <small>{opened.community.membersCount}</small></button>
            </>
          ) : (
            <>
              {sections.map(item => <a key={item.href} className={`channel ${item.active ? "selected" : ""}`} aria-current={item.active ? "page" : undefined} href={item.href}><Icon name={item.icon} />{item.label}{item.href === "#/friends" && friendUnread > 0 && <b className="unread">{friendUnread}</b>}</a>)}
              <a className={`channel ${route === "#/events/rsvps" ? "selected" : ""}`} aria-current={route === "#/events/rsvps" ? "page" : undefined} href="#/events/rsvps"><Icon name="calendar" /> My RSVPs</a>
              <a className={`channel ${isNotifications ? "selected" : ""}`} aria-current={isNotifications ? "page" : undefined} href="#/notifications"><Icon name="bell" /> Notifications {activity.items.some(x => !x.read) && <b className="unread">{activity.items.filter(x => !x.read).length}</b>}</a>
              <a className={`channel ${isInvitations ? "selected" : ""}`} aria-current={isInvitations ? "page" : undefined} href="#/invitations"><Icon name="invitation" /> Invitations & requests</a>

            </>
          )}
          {!identity && <button className="primary mobile" disabled={connecting} onClick={() => void loadIdentity(true)}>
            {connecting ? "Connecting\u2026" : "Connect wallet"}
          </button>}
          {identity && <button className="mobile" onClick={forgetWalletSession}>Forget wallet session</button>}
          <div id="sidebar-voice" />
          {opened && (
            <div className="self">
              {identity && <Avatar wallet={identity.address} />}
              <div>
                {identity ? <Name wallet={identity.address} /> : "Wallet"}
                <small>Online</small>
              </div>
            </div>
          )}
          <a
            className="mobile drawer-jump"
            href="https://decentraland.org/jump"
            target="_blank"
            rel="noreferrer"
          >
            Jump in &#x2197;
          </a>
        </section>
      </aside>
      <main inert={drawer || undefined}>
        <header className={`conversation-header ${isEvents || isWorlds ? "events-header" : ""}`}>
          <button
            className="mobile"
            aria-label="Open navigation"
            ref={menuButton}
            aria-expanded={drawer}
            aria-controls="navigation"
            onClick={() => setDrawer(true)}
          >
            &#x2630;
          </button>
          <span className="hash">{opened ? "#" : <Icon name={sections.find(item => item.active)?.icon || (isEvents ? "calendar" : isNotifications ? "bell" : isInvitations ? "invitation" : "home")} />}</span>
          <div>
            <strong>
              {isHome ? "Home" : isNotifications ? "Notifications" : isEvents ? (route === "#/events/rsvps" ? "My RSVPs" : "Events") : isWorlds ? "Worlds" : isInvitations ? "Invitations & requests" : isNearby ? "Nearby" : isFriends ? "Friends" : isVoice ? "The lounge" : opened ? channel : isMine ? "My communities" : "Explore communities"}
            </strong>
            {opened && <small>{opened.community.name}</small>}
          </div>
          {isEvents && <div className="events-header-actions"><input type="search" aria-label="Search events" placeholder="Find an event" value={eventSearch} onChange={e => setEventSearch(e.target.value)} /><button className="primary" onClick={() => setDialog("event")}>Create an Event</button></div>}
          {isWorlds && <div className="events-header-actions"><input type="search" aria-label="Search worlds" placeholder="Find a world" value={worldSearch} onChange={e => setWorldSearch(e.target.value)} /></div>}
          {(route === "#/discover" || isMine) && !opened && <input className="community-discovery-search" type="search" aria-label="Find a community" placeholder="Find a community" value={search} onChange={e => setSearch(e.target.value)} />}
          {(route === "#/discover" || isMine) && !opened && (
            <button
              className="refresh-list"
              aria-label="Refresh communities"
              disabled={listLoading || busy || (isMine && !identity)}
              onClick={() => (isMine ? void loadMine() : void loadPublic())}
            >
              &#x21bb;
            </button>
          )}
          {opened && !isVoice && (
            <>
              <div className="conversation-tools"><button aria-label="Search conversation" aria-pressed={panel === "search"} onClick={() => setPanel(panel === "search" ? null : "search")}><Icon name="search" /></button><button aria-label="Pinned messages" aria-pressed={panel === "pins"} onClick={() => setPanel(panel === "pins" ? null : "pins")}><Icon name="pin" /></button><button aria-label="Community members" aria-pressed={panel === "members"} onClick={() => setPanel(panel === "members" ? null : "members")}><Icon name="people" /></button></div>
              <button
                disabled={busy}
                onClick={() =>
                  channel !== "announcements" || expired
                    ? void openCommunity(opened.community, channel)
                    : void announcements()
                }
                aria-label="Refresh conversation"
              >
                &#x21bb;
              </button>
            </>
          )}
        </header>
        <div id="mobile-voice" />
        {!online && <div className="offline-banner" role="status">You&#x2019;re offline. Your draft stays here until you reconnect.</div>}
        {error && (
          <div className="error" role="alert">
            {error}
            <button aria-label="Dismiss error" onClick={() => setError("")}>
              &#xd7;
            </button>
          </div>
        )}
        {busy && (
          <div className="progress" role="status">
            Updating&#x2026;
          </div>
        )}
        {friendsStarted && identity && <Suspense fallback={isFriends ? <p className="empty">Connecting friends&#x2026;</p> : null}><Friends key={identity.address.toLowerCase()} identity={identity} route={route} active={isFriends || isNearby || (isVoice && !!opened)} canStartVoice={canPublish} onUnread={setFriendUnread} onState={setFriendSnapshot} preferences={generalPreferences} /></Suspense>}
        {identity && <FriendRequestActivity wallet={identity.address} requests={friendSnapshot.requests} ready={friendSnapshot.ready} onNotify={activity.notify} />}
        {identity && <EventActivity identity={identity} friendsState={friendSnapshot} onNotify={activity.notify} />}
        {isHome ? <Home identity={identity} friends={friendSnapshot} communities={communities} joined={mine} loading={listLoading} error={listError} onConnect={() => void loadIdentity(true)} onRetry={() => void loadPublic()} onCreate={() => { if (identity) setEditing("create"); else { setCreateAfterConnect(true); void loadIdentity(true); } }} />
          : isNotifications ? <ActivityInbox items={activity.items} markRead={activity.markRead} clear={activity.clear} />
          : isEvents ? <EventsPage search={eventSearch} mine={route === "#/events/rsvps"} friendsState={friendSnapshot} identity={identity} onConnect={() => void loadIdentity(true)} />
          : isWorlds ? <WorldsPage search={worldSearch} />
          : isInvitations ? identity ? <CommunityInbox identity={identity} onConnect={() => void loadIdentity(true)} onOpen={c => go(channelPath(c.id))} onChanged={() => void loadMine()} /> : <section className="empty"><h1>Your invitations, all together.</h1><p>Connect to see community invitations and your pending requests.</p><button className="primary" disabled={connecting} onClick={() => void loadIdentity(true)}>Connect wallet</button></section>
          : isFriends || isNearby ? !identity && <section className="empty"><h1>Your friends, together.</h1><p>Connect to see your Decentraland friends and chat.</p><button className="primary" disabled={connecting} onClick={() => void loadIdentity(true)}>{connecting ? "Connecting\u2026" : "Connect wallet"}</button></section> : isVoice && opened ? null : !opened ? (
          <section className="discover">
            {route === "#/discover" && <LiveCommunities identity={identity} onConnect={() => void loadIdentity(true)} onJoin={id => go(channelPath(id,"voice"))} />}
            {!route.startsWith("#/c/") && !isMine && <div className="discovery-hero"><div><h1>Find your kind<br />of people.</h1><p>Good company. Shared worlds.<br />A conversation worth coming back to.</p></div><img src={`${basePath}assets/discovery-avatars.png`} alt="Decentraland avatars together" /></div>}
            {preview && <div className="community-banner"><Picture src={communityImage(preview.id)} fallback={preview.name.slice(0,2)} /></div>}
            <h1 className={!route.startsWith("#/c/") && !isMine ? "discovery-heading" : ""}>
              {route.startsWith("#/c/")
                ? preview?.name || "Open the conversation."
                : isMine
                  ? "Your communities."
                  : "Explore communities"}
            </h1>
            <p>
              {route.startsWith("#/c/")
                ? preview ? preview.description : identity ? "Opening community\u2026" : "Connect your wallet to continue."
                : isMine
                  ? "Pick up where you left off."
                  : "Keep the conversation going, wherever you are."}
            </p>
            {(route.startsWith("#/c/") || isMine) && !identity && (
              <div className="empty">
                <p>Authorize a 24-hour Decentraland session to chat.</p>
                <button className="primary" disabled={connecting} onClick={() => void loadIdentity(true)}>
                  {connecting ? "Connecting\u2026" : "Connect wallet"}
                </button>
                <a href="#/discover">Browse communities</a>
              </div>
            )}
            {preview && identity && (
              <div className="empty membership-actions">
                <button className="primary" disabled={busy || joinPending[preview.id] || preview.pendingRequestToJoin} onClick={() => void joinCommunity(preview)}>
                  {joinPending[preview.id] || preview.pendingRequestToJoin ? "Request sent" : busy ? "Joining\u2026" : preview.privacy === "private" ? "Request to join" : "Join community"}
                </button>
                {(joinPending[preview.id] || preview.pendingRequestToJoin) && <PendingCommunityRequest identity={identity} community={preview} onChanged={() => { setJoinPending(all => ({...all,[preview.id]:false})); void openCommunity(preview); }} />}
                <button className="outline-button" onClick={() => void openCommunity(preview)} disabled={busy}>Refresh membership</button>
              </div>
            )}
            {route.startsWith("#/c/") && identity && !preview && !busy && (
              <button
                className="primary"
                onClick={() => {
                  const match = route.split("/");
                  void openCommunity(
                    {
                      id: match[2],
                      name: "Community",
                      description: "",
                      membersCount: 0,
                    },
                    match[3],
                  );
                }}
              >
                Retry opening chat
              </button>
            )}
            {listError && !isMine && (
              <div className="empty" role="alert">
                <p>{listError}</p>
                <button onClick={() => void loadPublic()}>
                  Retry loading communities
                </button>
              </div>
            )}
            {listLoading && !isMine && (
              <p role="status">Loading communities&#x2026;</p>
            )}
            <div
              className="cards"
              hidden={route.startsWith("#/c/") || (isMine && !identity)}
            >
              {visible.map((c, i) => (
                <button
                  key={c.id}
                  className="community-card"
                  onClick={() => go(channelPath(c.id))}
                >
                  <div className={`cover color-${i % 4}`}>
                    <Picture src={c.thumbnails?.raw || communityImage(c.id)} fallback={c.name.slice(0, 2)} />
                  </div>
                  <div className="card-content">
                    <h2>{c.name}</h2>
                    <p>{c.description}</p>
                    <small>
                      <i /> {c.membersCount} members
                    </small>
                    <span className="card-cta">Take a look <span>&#x2192;</span></span>
                  </div>
                </button>
              ))}
            </div>
            {!visible.length &&
              !listLoading &&
              !listError &&
              !busy &&
              !route.startsWith("#/c/") &&
              (!isMine || identity) && (
                <div className="empty">
                  <p>
                    {search
                      ? "No communities match your search."
                      : isMine
                        ? "You haven\u2019t joined a community yet."
                        : "No communities to show."}
                  </p>
                  {search ? (
                    <button onClick={() => setSearch("")}>Clear search</button>
                  ) : isMine ? (
                    <a href="#/discover">Discover communities</a>
                  ) : (
                    <button onClick={() => void loadPublic()}>Try again</button>
                  )}
                </div>
              )}
            {!isMine &&
              !route.startsWith("#/c/") &&
              communities.length < total && (
                <button
                  className="load-more"
                  disabled={listLoading}
                  onClick={() => void loadPublic(communities.length)}
                >
                  Load more communities
                </button>
              )}
          </section>
        ) : (
          <>
            <div className="chat-workspace"><div className="chat-column">
            {messages.some(m => m.pinned) && channel !== "announcements" && <button className="pinned-strip" onClick={() => setPanel("pins")}><Icon name="pin" /><span>{messages.find(m => m.pinned)?.text || "Pinned scene"}</span><small>View</small></button>}
            <section
              className="messages"
              aria-label={channel !== "announcements" ? "Messages" : "Announcements"}
              ref={scroller}
              onScroll={() => {
                const el = scroller.current;
                if (el)
                  followMessages.current =
                    el.scrollHeight - el.scrollTop - el.clientHeight < 80;
              }}
            >
              {channel!=="announcements"&&historyMore&&<button className="outline-button history-more" disabled={olderBusy} onClick={()=>void older()}>{olderBusy?"Loading\u2026":"Load older messages"}</button>}
              <div className={`welcome ${messages.length && channel !== "announcements" ? "has-messages" : ""}`}>
                <div className="welcome-icon">
                  {channel !== "announcements" ? "#" : "\u2301"}
                </div>
                <h1>
                  {channel !== "announcements"
                    ? messages.length
                      ? `Welcome to #${channel}.`
                      : "The conversation starts here."
                    : "From your community."}
                </h1>
                <p>
                  {channel !== "announcements"
                    ? `Welcome to ${opened.community.name}.`
                    : "Announcements shared with Decentraland."}
                </p>
              </div>
              {channel !== "announcements"
                ? messages.map((m) => (
                    <article className="message" key={m.id}>
                      <Avatar wallet={m.wallet} />
                      <div>
                        <header>
                          <strong>
                            <Name wallet={m.wallet} />
                          </strong>
                          <time dateTime={new Date(m.createdAt).toISOString()}>
                            {new Date(m.createdAt).toLocaleTimeString([], {
                              hour: "2-digit",
                              minute: "2-digit",
                            })}
                          </time>
                        </header>
                        <p>{m.text}</p>
                        <div className="message-actions"><button disabled={busy||reported.includes(m.id)} aria-label={reported.includes(m.id)?"Message reported":"Report message"} onClick={()=>void messageAction(m,"report")}>&#x2691;</button><button disabled={busy} aria-label="React with heart" onClick={() => void messageAction(m, "heart")}>&#x2661;</button><button disabled={busy} aria-label="Celebrate message" onClick={() => void messageAction(m, "celebrate")}>&#x2726;</button><button aria-label="Reply to message" onClick={() => { setThread(m.id); setThreadMessage(m); setPanel("thread"); }}><Icon name="reply" /></button>{canPublish && <button disabled={busy} aria-label={m.pinned ? "Unpin message" : "Pin message"} onClick={() => void messageAction(m, m.pinned ? "unpin" : "pin")}><Icon name="pin" /></button>}</div>
                        {!!m.reactions?.length && <div className="reactions">{(["heart", "celebrate"] as const).map(action => { const reactions = m.reactions!.filter(r => r.emoji === action); return reactions.length ? <button key={action} disabled={busy} aria-pressed={reactions.some(r => r.wallet === identity?.address.toLowerCase())} onClick={() => void messageAction(m, action)}>{action === "heart" ? "\u2661" : "\u2726"} {reactions.length}</button> : null; })}</div>}
                        {!!m.replies?.length && <button className="thread-link" onClick={() => { setThread(m.id); setThreadMessage(m); setPanel("thread"); }}>{m.replyCount??m.replies.length} {(m.replyCount??m.replies.length) === 1 ? "reply" : "replies"} <span>&#x2192;</span></button>}
                        {m.scene && <PlaceCard scene={m.scene} />}
                      </div>
                    </article>
                  ))
                : posts.map((p) => (
                    <article className="message post" key={p.id}>
                      <Avatar wallet={p.authorAddress} />
                      <div>
                        <header>
                          <strong>
                            {p.authorName || shortWallet(p.authorAddress)}
                          </strong>
                          <time>
                            {new Date(p.createdAt).toLocaleDateString()}
                          </time>
                        </header>
                        <p>{p.content}</p>
                      </div>
                    </article>
                  ))}
              {expired && (
                <div className="empty">
                  <p>Refresh to continue reading.</p>
                  <button
                    className="primary"
                    disabled={busy}
                    onClick={() =>
                      void openCommunity(opened.community, channel)
                    }
                  >
                    Refresh chat
                  </button>
                </div>
              )}
              <div ref={bottom} />
            </section>
            {((channel !== "announcements" && !opened.channelConfig?.read_only) || canPublish) && (
              <form className="composer" onSubmit={send}>
                {scene && (
                  <div className="attachment">
                    &#x25c7; {destinationLabel(scene)}
                    <button
                      disabled={busy}
                      type="button"
                      aria-label="Remove scene"
                      onClick={() => setScene(null)}
                    >
                      &#xd7;
                    </button>
                  </div>
                )}
                <div className="composer-row">
                  {channel !== "announcements" && (
                    <button
                      disabled={busy || expired}
                      type="button"
                      aria-label="Share a scene"
                      onClick={() => setPicker(true)}
                    >
                      <Icon name="plus" />
                    </button>
                  )}
                  <textarea
                    ref={composerInput}
                    disabled={busy || expired}
                    aria-label="Message"
                    placeholder={
                      channel !== "announcements"
                        ? `Message #${channel}`
                        : "Share an announcement"
                    }
                    maxLength={channel !== "announcements" ? 4000 : 1000}
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (
                        e.key === "Enter" &&
                        !e.shiftKey &&
                        !e.nativeEvent.isComposing
                      ) {
                        e.preventDefault();
                        e.currentTarget.form?.requestSubmit();
                      }
                    }}
                  />
                  <div className="emoji-control"><button type="button" aria-label="Add emoji" aria-expanded={emoji} onClick={() => setEmoji(v => !v)}><Icon name="smile" /></button>{emoji && <div className="emoji-picker">{["\u{1f60a}","\u2764\ufe0f","\u{1f389}","\u{1f44d}","\u{1f525}","\u{1f44b}","\u2728","\u{1f334}"].map(value => <button key={value} type="button" onClick={() => { setDraft(draft + value); setEmoji(false); composerInput.current?.focus(); }}>{value}</button>)}</div>}</div>
                  <button
                    className="send"
                    aria-label={
                      channel !== "announcements"
                        ? "Send message"
                        : "Publish announcement"
                    }
                    disabled={busy || expired || (!draft.trim() && !scene)}
                  >
                    <Icon name="send" />
                  </button>
                </div>
              </form>
            )}
            {!canPublish && (channel==="announcements" || opened.channelConfig?.read_only) && <p className="panel-empty">Only moderators can post here.</p>}
            </div>
            {panel && identity && <ConversationPanel key={`${opened.community.id}/${panel}/${thread}`} mode={panel} community={opened.community} memberStatuses={friendSnapshot.communityStatuses?.[opened.community.id]} identity={identity} opened={opened} parent={messages.find(m => m.id === thread)||threadMessage} onRefresh={()=>void openCommunity(opened.community,channel)} onClose={() => setPanel(null)} onThread={m => { setThread(m.id);setThreadMessage(m);setPanel("thread"); }} onReply={async text => { const current = generation.current, o = opened; await execute(identity, { type: "reply", community_id: o.community.id, message_id: thread, text }, () => current === generation.current); if (current === generation.current) await readHistory(o); }} />}
            </div>
          </>
        )}
      </main>
      {isVoice && panel && opened && identity && <Dialog title="Community" onClose={() => setPanel(null)}><ConversationPanel mode={panel} community={opened.community} memberStatuses={friendSnapshot.communityStatuses?.[opened.community.id]} identity={identity} opened={opened} onRefresh={()=>void openCommunity(opened.community,channel)} onClose={() => setPanel(null)} onThread={() => { setPanel(null); go(channelPath(opened.community.id)); }} onReply={async () => {}} /></Dialog>}
      {editing && identity && <CommunityEditor key={editing} community={editing === "settings" ? opened?.community : undefined} identity={identity} onClose={() => setEditing(null)} onSaved={c => { setEditing(null); if (c.id) { if (opened?.community.id === c.id) void openCommunity(c, channel); else go(channelPath(c.id)); void loadPublic(); } }} />}
      {profile && <ProfileDialog wallet={profile} self={identity?.address} onClose={() => setProfile("")} />}
      {dialog === "requests" && identity && opened && <JoinRequests community={opened.community} identity={identity} onClose={() => setDialog(null)} />}
      {dialog === "leave" && opened && identity && <Dialog title="Leave community" onClose={()=>setDialog(null)}><div className="dialog-content"><h2>Leave {opened.community.name}?</h2><p>You can rejoin public communities or request access again.</p><button className="primary" disabled={busy} onClick={()=>void run(async()=>{await execute(identity,{type:"manage_community",community_id:opened.community.id,action:{kind:"leave"}});setMine(all=>all.filter(c=>c.id!==opened.community.id));setDialog(null);go("#/friends");void loadPublic();})}>Leave community</button></div></Dialog>}
      {dialog === "channelSettings" && identity && opened && <ChannelSettings opened={opened} identity={identity} onClose={()=>setDialog(null)} onSaved={()=>{setDialog(null);void openCommunity(opened.community,channel);}}/>}
      {dialog === "channel" && identity && opened && <Dialog title="Create channel" onClose={() => setDialog(null)}><form className="dialog-content community-editor" onSubmit={async e => { e.preventDefault(); if (busy) return; setChannelError(""); const current = generation.current, o = opened; setBusy(true); try { await execute(identity,{type:"create_channel",community_id:o.community.id,name:channelName,private:privateChannel},() => current === generation.current); if (current === generation.current) { setDialog(null); go(channelPath(o.community.id,channelName)); } } catch(e) { if (current === generation.current) setChannelError(e instanceof Error ? e.message : "Channel could not be created"); } finally { if (current === generation.current) setBusy(false); } }}><h2>A new conversation.</h2><label>Channel name<input autoFocus required pattern="[a-z0-9-]{1,32}" maxLength={32} placeholder="world-building" value={channelName} onChange={e => setChannelName(e.target.value.toLowerCase().replace(/ /g,"-"))} /></label><label className="preference-row"><span>Only owners and moderators</span><input type="checkbox" checked={privateChannel} onChange={e => setPrivateChannel(e.target.checked)} /></label>{channelError && <p role="alert">{channelError}</p>}<button className="primary" disabled={busy}>Create channel</button></form></Dialog>}
      {dialog === "invite" && opened && identity && <Invite community={opened.community} identity={identity} onClose={()=>setDialog(null)}/>}
      {dialog === "hangouts" && opened && identity && <Dialog title="Community hangouts" onClose={() => setDialog(null)}><CommunityHangouts community={opened.community} identity={identity} /></Dialog>}
      {dialog === "event" && <EventEditor identity={identity} onConnect={() => void loadIdentity(true)} onClose={() => setDialog(null)} />}
      {dialog === "preferences" && !identity && <Dialog title="Preferences" onClose={() => setDialog(null)}><div className="dialog-content"><h2>Preferences</h2>{generalPreferences}</div></Dialog>}
      {picker && (
        <ScenePicker
          onClose={() => {
            setPicker(false);
            composerInput.current?.focus();
          }}
          onSelect={(s) => {
            setScene(s);
            setPicker(false);
            composerInput.current?.focus();
          }}
        />
      )}
    </div>
  );
}
createRoot(document.getElementById("root")!).render(<App />);

import "./mockup.css";
