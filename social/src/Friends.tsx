import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { connectFriends, sharedScene, jumpUrl, type DirectMessage, type FriendsState, type FriendshipAction, type Friend } from "./friends";
import { shortWallet, basePath, type WalletIdentity } from "./api";
import { Picture } from "./Picture";
import { Call } from "./Call";
import { notifyMessage } from "./notifications";
import { Voice } from "./Voice";
import { emptyVoice } from "./voice";
import { PlaceCard } from "./PlaceCard";
import { Dialog } from "./Dialog";
import { Avatar } from "./Profile";
import { Icon } from "./Icon";
import { ScenePicker } from "./ScenePicker";
import "./friends.css";
import { SocialSettings } from "./SocialSettings";
import { FriendsMap } from "./FriendsMap";
import type { SocialSettings as Privacy } from "@dcl/protocol/out-js/decentraland/social_service/v2/social_service_v2.gen";
import { destinationLabel } from "./destinations";

export function Friends({ identity, route, active, onUnread, canStartVoice = false, onState, preferences }: { preferences: ReactNode; onState?: (state: FriendsState) => void; canStartVoice?: boolean; identity: WalletIdentity; route: string; active: boolean; onUnread: (count: number) => void }) {
  const [settingsTab,setSettingsTab] = useState<"general"|"audio"|"privacy"|null>(null);
  const stateCallback = useRef(onState);
  stateCallback.current = onState;
  const [mutual, setMutual] = useState<Friend[] | null>(null);
  const [mutualLoading, setMutualLoading] = useState(false);
  const [newMessage, setNewMessage] = useState(false);
  const [newAddress, setNewAddress] = useState("");
  const [requested, setRequested] = useState(false);
  const [filter, setFilter] = useState<"all" | "unread" | "requests" | "blocked">("all");
  const [friendBusy, setFriendBusy] = useState(false);
  const [friendError, setFriendError] = useState("");
  const [state, setState] = useState<FriendsState>({ friends: [], ready: false, error: "" });
  const [messages, setMessages] = useState<DirectMessage[]>([]);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [unread, setUnread] = useState<Record<string, number>>({});
  const [search, setSearch] = useState("");
  const [retry, setRetry] = useState(0);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState("");
  const [picker, setPicker] = useState(false);
  const connection = useRef<ReturnType<typeof connectFriends> | null>(null);
  const end = useRef<HTMLDivElement>(null);
  const loadSettings = useCallback(async()=>{if(!connection.current)throw new Error('Connect to Decentraland first.');return connection.current.settings();},[]);
  const saveSettings = useCallback(async(settings:Privacy)=>{if(!connection.current)throw new Error('Connect to Decentraland first.');return connection.current.settings(settings);},[]);
  const selectDevice = useCallback(async(kind:'audioinput'|'audiooutput',id:string)=>{await connection.current?.voice.device(kind,id);},[]);
  useEffect(()=>{const open=(event:Event)=>setSettingsTab((event as CustomEvent).detail==='audio'?'audio':(event as CustomEvent).detail==='general'?'general':'privacy');window.addEventListener('social:settings',open);return()=>window.removeEventListener('social:settings',open);},[]);
  const voiceCommunity = route.match(/^#\/c\/([0-9a-f-]{36})\/voice$/i)?.[1] || "";
  const selected = route.startsWith("#/dm/") ? route.slice(5).toLowerCase() : "";
  const selectedRef = useRef(selected);
  selectedRef.current = active ? selected : "";
  const presenceRank = (f: FriendsState["friends"][number]) => f.available || f.status === 0 ? 0 : f.status === 2 ? 1 : 2;
  const sortedFriends = [...state.friends].sort((a,b) => presenceRank(a)-presenceRank(b) || (a.name||a.address).localeCompare(b.name||b.address));
  const friend = state.friends.find(f => f.address.toLowerCase() === selected);
  const canSend = state.ready && !!friend;
  const thread = messages.filter(m => m.peer === selected);
  const lastPlace = [...thread].reverse().find(m => m.sender === selected && sharedScene(m.text));
  useEffect(() => {
    let current = true;
    setState({ friends: [], ready: false, error: "" });
    const client = connectFriends(identity, next => { if (current) { setState(next); stateCallback.current?.(next); } }, message => {
      if (!current) return;
      setMessages(all => [...all, message].slice(-1000));
      if (message.sender !== identity.address.toLowerCase() && (selectedRef.current !== message.peer || document.hidden)) { setUnread(all => ({ ...all, [message.peer]: (all[message.peer] || 0) + 1 })); notifyMessage(shortWallet(message.sender),message.peer); }
    });
    connection.current = client;
    return () => { current = false; client.close(); connection.current = null; };
  }, [identity, retry]);
  useEffect(() => { const call = (e: Event) => { void connection.current?.voice.call((e as CustomEvent<string>).detail)?.catch(error => setFriendError(error.message)); };window.addEventListener("social:call",call);return()=>window.removeEventListener("social:call",call); },[]);
  useEffect(() => { if (route === "#/friends/requests") setFilter("requests"); else if (route === "#/friends/blocked") setFilter("blocked"); }, [route]);
  useEffect(() => { const open = () => { setNewMessage(true); setRequested(false); setFriendError(""); }; window.addEventListener("social:add-friend",open); return () => window.removeEventListener("social:add-friend",open); }, []);
  useEffect(() => { onUnread(Object.values(unread).reduce((a, b) => a + b, 0)); }, [unread, onUnread]);
  useEffect(() => { setError(""); setRequested(false); setPicker(false); if (active && selected) setUnread(all => ({ ...all, [selected]: 0 })); }, [selected, active]);
  useEffect(() => { if (active) end.current?.scrollIntoView({ block: "nearest" }); }, [messages, selected, active]);
  async function send(text = drafts[selected] || "") {
    if (sending || !text.trim() || !selected) return;
    const peer = selected;
    const client = connection.current;
    if (!client) return;
    setSending(true); setError("");
    try {
      await client.send(peer, text);
      setDrafts(all => ({ ...all, [peer]: all[peer] === text ? "" : all[peer] }));
    } catch (e) { setError(e instanceof Error ? e.message : "Message could not be sent."); }
    finally { setSending(false); }
  }
  async function friendship(action: FriendshipAction, address: string) {
    if (friendBusy || !connection.current) return;
    setFriendBusy(true); setFriendError("");
    try { await connection.current.friendship(action, address); if (action === "request") setRequested(true); }
    catch(e) { setFriendError(e instanceof Error ? e.message : "Could not update friendship."); }
    finally { setFriendBusy(false); }
  }
  async function block(address: string, value: boolean) {
    if (friendBusy || !connection.current) return;
    setFriendBusy(true); setFriendError("");
    try { await connection.current.block(address,value); }
    catch (e) { setFriendError(e instanceof Error ? e.message : "Could not update block."); }
    finally { setFriendBusy(false); }
  }
  async function showMutual() {
    if (!connection.current) return;
    setMutual([]); setMutualLoading(true); setFriendError("");
    try { setMutual((await connection.current.mutual(selected)).map(f => ({...f,available:false,location:null}))); }
    catch (e) { setFriendError(e instanceof Error ? e.message : "Could not load mutual friends."); }
    finally { setMutualLoading(false); }
  }
  const settings = settingsTab && <SocialSettings general={preferences} tab={settingsTab} onClose={()=>setSettingsTab(null)} load={loadSettings} save={saveSettings} device={selectDevice}/>;
  const call = <Call route={route} state={state.voice || emptyVoice} accept={() => {void connection.current?.voice.accept();}} leave={() => {void connection.current?.voice.leave();}} mute={() => {void connection.current?.voice.mute();}} deafen={() => connection.current?.voice.deafen()} />;
  const nearby = <section className="nearby-page"><div className="nearby-art" style={{backgroundImage:`linear-gradient(0deg,#161518,transparent 80%),url(${basePath}assets/nearby-world.webp)`}} /><div className="nearby-content"><FriendsMap friends={state.friends}/>{state.friends.some(f=>f.location) && <div className="nearby-friends"><h2>Friends out exploring</h2>{sortedFriends.filter(f=>f.location).map(f=><div className="nearby-friend" key={f.address}><Avatar wallet={f.address} /><div><strong>{f.name || shortWallet(f.address)}</strong><small>{destinationLabel(f.location!)}</small></div><a className="outline-button" href={jumpUrl(f.location!)} target="_blank" rel="noreferrer">Join &#x2197;</a><a className="outline-button" href={`#/dm/${f.address.toLowerCase()}`}>Message</a></div>)}</div>}</div></section>;
  if (route === "#/nearby" && active) return <>{call}{settings}{nearby}</>;
  if (voiceCommunity && active) return <>{call}{settings}<Voice state={state.voice || emptyVoice} community={voiceCommunity} self={identity.address} localMute={address=>connection.current?.voice.localMute(address)} moderate={(action,address)=>{void connection.current?.voice.moderate(action,address);}} canStart={canStartVoice} join={start => { void connection.current?.voice.join(voiceCommunity,start); }} leave={() => { void connection.current?.voice.leave(); }} mute={() => { void connection.current?.voice.mute(); }} deafen={() => connection.current?.voice.deafen()} hand={() => { void connection.current?.voice.hand(); }} /></>;
  return <>{call}{settings}<section hidden={!active} className={`friends-view ${!selected && filter === "all" && !messages.length ? "nearby-empty" : ""} ${selected ? "has-peer" : ""}`} aria-label="Friends">
    <aside className="friends-list"><div className="friends-scroll">
      <input aria-label="Find a friend" placeholder="Find a friend" value={search} onChange={e => setSearch(e.target.value)} />
      {!state.ready && !state.error && <p className="muted">Connecting to Decentraland&#x2026;</p>}
      {state.error && <div role="alert" className="friends-error"><p>{state.error}</p><button onClick={() => setRetry(n => n + 1)}>Reconnect</button></div>}
      {state.ready && state.friends.length === 0 && <p className="muted">Your Decentraland friends will appear here.</p>}
      {sortedFriends.filter(f => `${f.name} ${f.address}`.toLowerCase().includes(search.toLowerCase())).map(f => <a className={`friend-row ${selected === f.address.toLowerCase() ? "selected" : ""}`} href={`#/dm/${f.address.toLowerCase()}`} key={f.address} aria-current={selected === f.address.toLowerCase() ? "page" : undefined}>
        <span className="friend-picture"><Picture src={f.profilePictureUrl} fallback={(f.name || f.address.slice(2)).slice(0, 2)} /><i className={f.available || f.status === 0 ? "online" : f.status === 2 ? "away" : ""} /></span>
        <span><strong>{f.name || shortWallet(f.address)}</strong><small>{f.available ? "Available to chat" : f.status === 0 ? "Online" : f.status === 1 ? "Offline" : f.status === 2 ? "Away" : "Status unavailable"}</small>{f.location && <small className="live-location">At {destinationLabel(f.location)}</small>}</span>
        {!!unread[f.address.toLowerCase()] && <b className="unread">{unread[f.address.toLowerCase()]}</b>}
      </a>)}
    </div><div id="friends-voice" /></aside>
    <section className="direct-chat">
      {selected ? <>
        <header className="direct-header"><button className="friends-back" aria-label="Back to friends" onClick={() => { window.location.hash = "#/friends"; }}>&#x2190;</button><Avatar wallet={selected} /><div><h2>{friend?.name || shortWallet(selected)}</h2><small>{friend?.location ? `At ${destinationLabel(friend.location)}` : friend?.available ? "Available to chat" : friend?.status === 0 ? "Online" : friend?.status === 2 ? "Away" : friend?.status === 1 ? "Offline" : state.ready ? "Status unavailable" : "Connecting\u2026"}</small></div>
        <details className="friend-actions"><summary aria-label="Friend options">&#x2022;&#x2022;&#x2022;</summary><div><button disabled={!state.ready} onClick={() => void showMutual()}>Mutual friends</button>{friend && <button disabled={friendBusy} onClick={() => void friendship("delete",selected)}>Remove friend</button>}<button disabled={friendBusy || !state.ready} onClick={() => void block(selected,!state.blockedUsers?.includes(selected))}>{state.blockedUsers?.includes(selected) ? "Unblock" : "Block"}</button></div></details><button className="outline-button" disabled={!friend || !state.ready} onClick={() => {void connection.current?.voice.call(selected);}}>Call</button>
        {friend?.location ? <a className="primary" href={jumpUrl(friend.location)} target="_blank" rel="noreferrer" title="Current Decentraland location">Join friend &#x2197;</a> : lastPlace && <a className="primary" href={jumpUrl(sharedScene(lastPlace.text)!)} target="_blank" rel="noreferrer" title="Open your friend&#x2019;s last shared place">Jump in &#x2197;</a>}</header>
        <div className="direct-history" role="log" aria-label="Direct messages" aria-live="polite">
          {!thread.length && <div className="empty"><h2>Say hello{friend?.name ? ` to ${friend.name}` : ""}.</h2><p>Messages are live. Both of you need to be connected.</p>{!friend && state.ready && <button className="outline-button" disabled={friendBusy || requested} onClick={() => void friendship("request",selected)}>{requested ? "Request sent" : "Add friend"}</button>}</div>}
          {thread.map(m => { const scene = sharedScene(m.text); return <article key={m.id} className={`direct-message ${m.sender === identity.address.toLowerCase() ? "own" : ""}`}><Avatar wallet={m.sender} /><div className="direct-message-content"><div className="direct-message-heading"><strong>{m.sender === identity.address.toLowerCase() ? "You" : friend?.name || shortWallet(m.sender)}</strong><time>{new Date(m.time).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</time></div>{scene ? <PlaceCard scene={scene} /> : <p>{m.text}</p>}</div></article>; })}<div ref={end} />
        </div>
        {error && <p role="alert" className="friends-error">{error}</p>}
        {state.error && <div className="friends-error" role="alert">{state.error} <button onClick={() => setRetry(n => n + 1)}>Reconnect</button></div>}
        <form className="direct-composer" onSubmit={e => { e.preventDefault(); void send(); }}><button type="button" aria-label="Share a place with friend" disabled={!canSend || sending} onClick={() => setPicker(true)}>+</button><textarea aria-label="Direct message" placeholder={canSend ? `Message ${friend?.name || shortWallet(selected)}` : state.ready ? "Add this person as a friend to chat" : "Connecting to chat\u2026"} disabled={!canSend} maxLength={4000} value={drafts[selected] || ""} onChange={e => setDrafts(all => ({ ...all, [selected]: e.target.value }))} onKeyDown={e => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); void send(); } }} /><button type="submit" aria-label="Send direct message" disabled={!canSend || sending || !drafts[selected]?.trim()}><Icon name="send" /></button></form>
      </> : <div className="inbox"><header><div className="inbox-header-actions"><a className="outline-button" href={state.ownLocation ? jumpUrl(state.ownLocation) : "https://decentraland.org/jump"} target="_blank" rel="noreferrer">Open Decentraland &#x2197;</a><button className="outline-button" onClick={() => { setNewMessage(true); setRequested(false); setFriendError(""); }}>Add friend <Icon name="plus" /></button></div></header><nav className="inbox-tabs" aria-label="Inbox filters">{(["all","unread","requests","blocked"] as const).map(value => <button key={value} aria-pressed={filter === value} onClick={() => setFilter(value)}>{value === "all" ? "All" : value === "unread" ? "Unread" : value === "requests" ? "Requests" : "Blocked"}{value === "requests" && !!state.requests?.length && <b className="unread">{state.requests.length}</b>}</button>)}</nav>
      {filter === "blocked" ? <><p className="muted">Blocked people cannot message or call you.</p>{state.blockedUsers?.map(address => <div className="request-row" key={address}><Avatar wallet={address} /><strong>{shortWallet(address)}</strong><button className="outline-button" disabled={friendBusy} onClick={() => void block(address,false)}>Unblock</button></div>)}{!state.blockedUsers?.length && <p className="inbox-empty">No blocked people.</p>}</> : filter === "requests" ? <>{state.requests?.map(r => <div className="request-row" key={r.id}><Avatar wallet={r.friend!.address} /><div><strong>{r.friend!.name || shortWallet(r.friend!.address)}</strong><p>{r.message || "Would like to be your friend."}</p></div><button className="primary" disabled={friendBusy} onClick={() => void friendship("accept",r.friend!.address)}>Accept</button><button disabled={friendBusy} onClick={() => void friendship("reject",r.friend!.address)}>Decline</button></div>)}<h3 className="friend-section-title">Sent requests</h3>{state.sentRequests?.map(r => <div className="request-row" key={r.id}><Avatar wallet={r.friend!.address} /><div><strong>{r.friend!.name || shortWallet(r.friend!.address)}</strong><p>Waiting for a reply</p></div><button className="outline-button" disabled={friendBusy} onClick={() => void friendship("cancel",r.friend!.address)}>Cancel request</button></div>)}{!state.sentRequests?.length && <p className="muted">No sent requests.</p>}{state.requestsError ? <p role="status" className="muted">{state.requestsError}</p> : !state.requests?.length && <p className="inbox-empty">You're all caught up.</p>}</> : filter === "all" && !messages.length ? nearby : <>{sortedFriends.filter(f => messages.some(m => m.peer === f.address.toLowerCase()) && (filter !== "unread" || unread[f.address.toLowerCase()])).map(f => { const last = messages.findLast(m => m.peer === f.address.toLowerCase()); return <a className="inbox-row" href={`#/dm/${f.address.toLowerCase()}`} key={f.address}><span className="friend-picture"><Picture src={f.profilePictureUrl} fallback={(f.name || f.address.slice(2)).slice(0,2)} /><i className={f.available || f.status === 0 ? "online" : f.status === 2 ? "away" : ""} /></span><div><strong>{f.name || shortWallet(f.address)}</strong><p>{last?.text || (f.location ? `Exploring ${destinationLabel(f.location)}` : f.available ? "Ready to chat" : "Say hello")}</p></div>{last && <time>{new Date(last.time).toLocaleTimeString([], {hour:"2-digit",minute:"2-digit"})}</time>}{!!unread[f.address.toLowerCase()] && <b className="unread">{unread[f.address.toLowerCase()]}</b>}</a>; })}{(filter === "unread" ? !Object.values(unread).some(Boolean) : state.ready && !state.friends.length) && <div className="inbox-empty"><Icon name="chat" /><p>{filter === "unread" ? "You're all caught up." : "Good conversations start with a hello."}</p>{filter === "all" && <button className="outline-button" onClick={() => setNewMessage(true)}>Find a friend</button>}</div>}</>}
    </div>}
    </section>
    {friendError && !newMessage && <p role="alert" className="friends-error">{friendError}</p>}
  </section>
    {mutual !== null && <Dialog title="Mutual friends" onClose={() => setMutual(null)}><div className="dialog-content"><h2>Friends in common</h2>{mutualLoading ? <p>Loading&#x2026;</p> : mutual.length ? mutual.map(f => <a className="friend-row" key={f.address} href={`#/dm/${f.address.toLowerCase()}`} onClick={() => setMutual(null)}><Avatar wallet={f.address} /><strong>{f.name || shortWallet(f.address)}</strong></a>) : <p className="muted">No mutual friends yet.</p>}{friendError && <p role="alert">{friendError}</p>}</div></Dialog>}
    {newMessage && <Dialog title="Add a friend" onClose={() => setNewMessage(false)}><div className="dialog-content"><h2>Start a conversation.</h2>{sortedFriends.map(f => <a key={f.address} className="friend-row" href={`#/dm/${f.address.toLowerCase()}`} onClick={() => setNewMessage(false)}><span className="friend-picture"><Picture src={f.profilePictureUrl} fallback={(f.name || f.address.slice(2)).slice(0,2)} /></span><strong>{f.name || shortWallet(f.address)}</strong></a>)}<form onSubmit={e => { e.preventDefault(); void friendship("request",newAddress); }}><label>Invite a friend<input aria-label="Friend wallet address" placeholder="0x&#x2026;" pattern="0x[0-9a-fA-F]{40}" required value={newAddress} onChange={e => { setNewAddress(e.target.value); setRequested(false); }} /></label><button className="primary" disabled={friendBusy || requested}>{requested ? "Request sent" : "Send friend request"}</button></form>{friendError && <p role="alert">{friendError}</p>}</div></Dialog>}
    {picker && <ScenePicker onClose={() => setPicker(false)} onSelect={scene => { setPicker(false); void send(jumpUrl(scene)); }} />}
  </>;
}
