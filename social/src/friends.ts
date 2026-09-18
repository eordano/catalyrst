import { createRpcClient } from "@dcl/rpc/dist/client";
import { loadService } from "@dcl/rpc/dist/codegen";
import { WebSocketTransport } from "@dcl/rpc/dist/transports/WebSocket";
import { SocialServiceDefinition, type FriendProfile, type FriendshipRequestResponse, type SocialSettings } from "@dcl/protocol/out-js/decentraland/social_service/v2/social_service_v2.gen";
import { Packet } from "@dcl/protocol/out-js/decentraland/kernel/comms/rfc4/comms.gen";
import { DisconnectReason, EngineEvent, Room, RoomEvent } from "livekit-client";
import { createVoice, emptyVoice, type VoiceState, type VoiceModeration } from "./voice";
import { freshLocation, LOCATION_TOPIC, type LiveLocation } from "./location";
import { worldName } from "./destinations";
import { reportError } from "./telemetry";
import { api, execute, type WalletIdentity, type Scene } from "./api";

export type Friend = FriendProfile & { status?: number; available: boolean; location: LiveLocation | null };
export type DirectMessage = { id: string; peer: string; sender: string; text: string; time: number };
export type FriendshipAction = "request" | "accept" | "reject" | "cancel" | "delete";
export type FriendsState = { communityStatuses?: Record<string, Record<string, number>>; blockedUsers?: string[]; sentRequests?: FriendshipRequestResponse[]; recovering?: boolean; retryable?: boolean; voice?: VoiceState; requests?: FriendshipRequestResponse[]; requestsError?: string; friends: Friend[]; ready: boolean; error: string; ownLocation?: LiveLocation | null };
export { destinationUrl as jumpUrl } from "./destinations";
export function sharedScene(text: string): Scene | null {
  try {
    const url = new URL(text.trim());
    if (url.origin !== "https://decentraland.org" || !/^\/jump\/?$/.test(url.pathname)) return null;
    const world = worldName(url.searchParams.get("realm") || "");
    if (world) return {x:0,y:0,world};
    const match = url.searchParams.get("position")?.match(/^(-?\d{1,3}),(-?\d{1,3})$/);
    if (!match || Math.abs(+match[1]) > 150 || Math.abs(+match[2]) > 150) return null;
    return { x: +match[1], y: +match[2] };
  } catch { return null; }
}
function openFriends(identity: WalletIdentity, update: (state: FriendsState) => void, receive: (message: DirectMessage) => void) {
  let voice: ReturnType<typeof createVoice> | undefined;
  let voiceState = {...emptyVoice};
  let readSettings: (()=>Promise<SocialSettings>) | undefined;
  let writeSettings: ((settings:SocialSettings)=>Promise<SocialSettings>) | undefined;
  let socket: WebSocket | undefined;
  let room: Room | undefined;
  let stopped = false;
  let ready = false;
  let failed = false;
  let retryable = true;
  let heartbeat: ReturnType<typeof setInterval> | undefined;
  let checking = false;
  let friends: FriendProfile[] = [];
  let requests: FriendshipRequestResponse[] = [];
  let requestsError = "";
  let sentRequests: FriendshipRequestResponse[] = [];
  let blockedUsers: string[] = [];
  let blockAction: ((address: string, block: boolean) => Promise<void>) | undefined;
  let mutualAction: ((address: string) => Promise<FriendProfile[]>) | undefined;
  let friendshipAction: ((action: FriendshipAction, address: string) => Promise<void>) | undefined;
  let blocked = new Set<string>();
  const statuses = new Map<string, number>();
  const communityStatuses: Record<string, Record<string, number>> = {};
  const seen = new Set<string>();
  const locations = new Map<string, LiveLocation>();
  let ownLocation: LiveLocation | null = null;
  let explorerLocations: Record<string, LiveLocation | null> = {};
  let polling = false;
  let lastPoll = 0;
  let timer: ReturnType<typeof setInterval> | undefined;
  const peerParticipant = (address: string) => [...(room?.remoteParticipants.values() || [])].find(p => p.identity.toLowerCase() === address.toLowerCase());
  const publish = (error = "", recovering = false) => { if (!stopped) update({ communityStatuses: ready ? {...communityStatuses} : {}, recovering, retryable, voice: voiceState, requests, sentRequests, blockedUsers, requestsError, friends: friends.filter(f => !blocked.has(f.address.toLowerCase())).map(f => ({ ...f, status: ready ? (freshLocation(explorerLocations[f.address.toLowerCase()]) ? 0 : statuses.get(f.address.toLowerCase())) : undefined, available: ready && !!peerParticipant(f.address), location: ready ? freshLocation(explorerLocations[f.address.toLowerCase()]) || (!!peerParticipant(f.address) ? freshLocation(locations.get(f.address.toLowerCase())) : null) : null })), ready, error, ownLocation: ready ? freshLocation(ownLocation) : null }); };
  const fail = (error: unknown) => { if (!stopped && !failed) { failed = true; ready = false; reportError(error, "friends"); publish(error instanceof Error ? error.message : "Friends connection lost."); close(); } };
  const close = () => { stopped = true; voice?.close(); if (heartbeat) clearInterval(heartbeat); if (timer) clearInterval(timer); locations.clear(); socket?.close(); void room?.disconnect(); };
  async function announceLocation() {
    if (!ready || !room || stopped) return;
    const targets = friends.filter(f => !blocked.has(f.address.toLowerCase())).map(f => peerParticipant(f.address)?.identity).filter((p): p is string => !!p);
    if (!targets.length) return;
    const bytes = new TextEncoder().encode(JSON.stringify({ location: freshLocation(ownLocation) }));
    await room.localParticipant.publishData(bytes, { reliable: true, topic: LOCATION_TOPIC, destinationIdentities: targets });
  }
  async function pollLocation() {
    if (stopped || !ready || polling) return;
    polling = true;
    lastPoll = Date.now();
    try {
      const addresses = friends.filter(f => !blocked.has(f.address.toLowerCase())).map(f => f.address.toLowerCase());
      const batches: string[][] = [];
      for (let i = 0; i < addresses.length; i += 100) batches.push(addresses.slice(i, i + 100));
      const [result, peers] = await Promise.all([
        execute<{ location: LiveLocation | null }>(identity, { type: "own_location" }, () => !stopped).catch(() => ({location:null})),
        Promise.all(batches.map(batch => api<{locations:Record<string,LiveLocation>}>(`/locations?addresses=${batch.join(',')}`).catch(() => ({locations:{}})))),
      ]);
      if (stopped || !ready) return;
      ownLocation = freshLocation(result.location);
      explorerLocations = Object.assign({}, ...peers.map(p => p.locations));
    } catch { ownLocation = null; explorerLocations = {}; }
    finally { polling = false; }
    if (!stopped && ready) { publish(); await announceLocation().catch(() => {}); }
  }
  const timeout = <T,>(promise: Promise<T>) => new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("Friends connection timed out. Try reconnecting.")), 15000);
    promise.then(value => { clearTimeout(timer); resolve(value); }, error => { clearTimeout(timer); reject(error); });
  });
  const start = async () => {
    const timestamp = String(Date.now());
    const metadata = "{}";
    const payload = `get:/:${timestamp}:${metadata}`;
    const chain = await identity.signRequest({ id: crypto.randomUUID(), wallet: identity.address, operation: { type: "social_connection" }, url: "wss://rpc-social-service-ea.decentraland.org/", method: "GET", body: null, payload, metadata, timestamp, expiresAt: Date.now() + 90000 });
    if (stopped) return;
    const headers: Record<string, string> = { "x-identity-timestamp": timestamp, "x-identity-metadata": metadata };
    chain.forEach((link, i) => { headers[`x-identity-auth-chain-${i}`] = JSON.stringify(link); });
    socket = new WebSocket("wss://rpc-social-service-ea.decentraland.org/");
    socket.addEventListener("open", () => socket?.send(new TextEncoder().encode(JSON.stringify(headers))), { once: true });
    socket.addEventListener("close", event => { if (!stopped) { retryable = event.code !== 3003; fail(new Error(event.code === 3003 ? "Wallet authorization rejected. Reconnect your wallet." : `Friends connection closed (code ${event.code}).`)); } });
    const client = await timeout(createRpcClient(WebSocketTransport(socket)));
    const port = await timeout(client.createPort("social"));
    const service = loadService(port, SocialServiceDefinition);
    if (stopped) return;
    readSettings = async () => { const result=await timeout(service.getSocialSettings({}));if(result.response?.$case!=='ok'||!result.response.ok.settings)throw new Error('Privacy settings are unavailable.');return result.response.ok.settings; };
    writeSettings = async settings => { const result=await timeout(service.upsertSocialSettings(settings));if(result.response?.$case!=='ok')throw new Error('Privacy settings could not be saved.');return result.response.ok; };
    heartbeat = setInterval(() => {
      if (stopped || checking) return;
      checking = true;
      void timeout(service.getFriendshipStatus({user:{address:identity.address}})).catch(fail).finally(()=>{checking=false;});
    },30000);
    voice = createVoice(service, state => { voiceState = state; publish(); }, identity.address, peer => friends.some(f => f.address.toLowerCase() === peer) && !blocked.has(peer));
    const refresh = async () => {
      const [block,first] = await Promise.all([timeout(service.getBlockingStatus({})),timeout(service.getFriends({pagination:{limit:100,offset:0}}))]);
      const next: FriendProfile[] = [];
      for (let offset = 0; offset < 10000; offset += 100) {
        const page = offset === 0 ? first : await timeout(service.getFriends({ pagination: { limit: 100, offset } }));
        next.push(...page.friends);
        if (page.friends.length < 100 || next.length >= (page.paginationData?.total ?? Infinity)) break;
      }
      blockedUsers = block.blockedUsers.map(address => address.toLowerCase());
      blocked = new Set([...block.blockedUsers, ...block.blockedByUsers].map(a => a.toLowerCase()));
      friends = next;
      voice?.enforceAccess();
      publish();
    };
    const refreshRequests = async () => {
      try {
        const load = async (sent: boolean) => {
          const all: FriendshipRequestResponse[] = [];
          for (let offset = 0; offset < 10000; offset += 100) {
            const payload = {pagination:{limit:100,offset}};
            const r = await timeout(sent ? service.getSentFriendshipRequests(payload) : service.getPendingFriendshipRequests(payload));
            if (r.response?.$case !== "requests") throw new Error("Friend requests are unavailable.");
            const page = r.response.requests.requests;
            all.push(...page.filter(r => r.friend && !blocked.has(r.friend.address.toLowerCase())));
            if (page.length < 100 || offset + page.length >= (r.paginationData?.total ?? Infinity)) break;
          }
          return all;
        };
        [requests, sentRequests] = await Promise.all([load(false), load(true)]);
        requestsError = "";
      } catch { requestsError = "Friend requests are unavailable. Reconnect to try again."; }
      publish();
    };
    friendshipAction = async (action, address) => {
      if (!/^0x[0-9a-f]{40}$/i.test(address) || address.toLowerCase() === identity.address.toLowerCase()) throw new Error("Enter another person's wallet address.");
      const user = {address:address.toLowerCase()};
      const result = await timeout(service.upsertFriendship({action: action === "request" ? {$case:"request",request:{user}} : action === "accept" ? {$case:"accept",accept:{user}} : action === "reject" ? {$case:"reject",reject:{user}} : action === "cancel" ? {$case:"cancel",cancel:{user}} : {$case:"delete",delete:{user}}}));
      if (result.response?.$case !== "accepted") throw new Error("The friendship action could not be completed. Refresh before trying again.");
      await refresh(); await refreshRequests();
    };
    blockAction = async (address, block) => {
      const user = {address:address.toLowerCase()};
      if (!/^0x[0-9a-f]{40}$/i.test(address) || user.address === identity.address.toLowerCase()) throw new Error("Choose another person's wallet.");
      const result = await timeout(block ? service.blockUser({user}) : service.unblockUser({user}));
      if (result.response?.$case !== "ok") throw new Error("Could not update this block. Try again.");
      await refresh(); await refreshRequests();
    };
    mutualAction = async address => {
      const all: FriendProfile[] = [];
      for (let offset = 0; offset < 10000; offset += 100) {
        const page = await timeout(service.getMutualFriends({user:{address},pagination:{limit:100,offset}}));
        all.push(...page.friends);
        if (page.friends.length < 100 || all.length >= (page.paginationData?.total ?? Infinity)) break;
      }
      return all;
    };
    await refresh();
    void refreshRequests();
    void voice.syncIncoming();
    if (stopped) return;
    const consume = async <T,>(stream: AsyncIterable<T>, handler: (value: T) => void | Promise<void>, required = true) => {
      try { for await (const event of stream) { if (stopped) return; await handler(event); } if (!stopped) throw new Error("Friends updates disconnected. Reconnect to continue."); } catch (error) {
        if (required) fail(error);
        else { for (const key of Object.keys(communityStatuses)) delete communityStatuses[key]; publish(); if (!stopped) reportError(error,"community-presence"); }
      }
    };
    void consume(service.subscribeToCommunityMemberConnectivityUpdates({}), event => {
      if (event.member) { communityStatuses[event.communityId] = {...communityStatuses[event.communityId], [event.member.address.toLowerCase()]: event.status}; publish(); }
    }, false);
    void consume(service.subscribeToFriendConnectivityUpdates({}), event => { if (event.friend) statuses.set(event.friend.address.toLowerCase(), event.status); publish(); });
    void consume(service.subscribeToFriendshipUpdates({}), async () => { ready = false; publish(); await refresh(); await refreshRequests(); ready = !!room && room.state === "connected"; publish(); });
    void consume(service.subscribeToBlockUpdates({}), async event => { if (event.isBlocked) blocked.add(event.address.toLowerCase()); publish(); await refresh(); });
    const token = await execute<{ adapter: string }>(identity, { type: "private_chat_token" }, () => !stopped);
    if (stopped) return;
    const adapter = new URL(token.adapter.replace(/^livekit:/, ""));
    if (adapter.protocol !== "wss:" || !adapter.searchParams.get("access_token")) throw new Error("Foundation returned an invalid private chat connection.");
    const accessToken = adapter.searchParams.get("access_token")!;
    adapter.search = "";
    room = new Room({ adaptiveStream: false, dynacast: false });
    room.on(RoomEvent.ParticipantConnected, () => { publish(); void announceLocation().catch(() => {}); });
    room.on(RoomEvent.ParticipantDisconnected, participant => { locations.delete(participant.identity.toLowerCase()); publish(); });
    room.on(RoomEvent.Reconnecting, () => { locations.clear(); ownLocation = null; ready = false; publish("Reconnecting chat\u2026",true); });
    room.on(RoomEvent.Reconnected, () => { ready = true; publish(); void pollLocation(); });
    room.on(RoomEvent.Disconnected, reason => {
      if (stopped) return;
      retryable = reason !== DisconnectReason.DUPLICATE_IDENTITY && reason !== DisconnectReason.PARTICIPANT_REMOVED;
      fail(new Error(reason === DisconnectReason.DUPLICATE_IDENTITY ? 'This wallet opened chat in another tab or Explorer. Close the other chat connection, then reconnect here.' : `Private chat disconnected (reason ${reason ?? 'unknown'}).`));
    });
    const receiveData = (bytes: Uint8Array, sender: string | undefined, topic?: string) => {
      const peer = sender?.toLowerCase();
      if (!ready || !peer || blocked.has(peer) || !friends.some(f => f.address.toLowerCase() === peer) || bytes.length > 20000) return;
      try {
        if (topic === LOCATION_TOPIC) {
          if (bytes.length > 512) return;
          const location = freshLocation(JSON.parse(new TextDecoder().decode(bytes)).location);
          if (location) locations.set(peer, location); else locations.delete(peer);
          publish();
          return;
        }
        // Explorer addresses private chat by both SFU destination and wallet topic.
        // Keep accepting legacy topic-less DMs, but never treat community traffic as a DM.
        if (topic && topic.toLowerCase() !== identity.address.toLowerCase()) return;
        const packet = Packet.decode(bytes);
        if (packet.message?.$case !== "chat") return;
        const chat = packet.message.chat;
        if (!chat.message.trim() || chat.message.length > 4000) return;
        const id = `${peer}:${chat.timestamp}:${chat.message}`;
        if (seen.has(id)) return;
        if (seen.size >= 1000) seen.delete(seen.values().next().value!);
        seen.add(id);
        receive({ id: crypto.randomUUID(), peer, sender: peer, text: chat.message, time: Date.now() });
      } catch { /* Ignore non-chat packets and malformed peer data. */ }
    };
    room.on(RoomEvent.DataReceived, (bytes,participant,_kind,topic) => receiveData(bytes,participant?.identity,topic));
    // Some data-only participants are announced after their first packet. The
    // SFU envelope carries their identity even when Room has no Participant yet.
    room.engine.on(EngineEvent.DataPacketReceived, packet => {
      if (packet.value?.case !== 'user' || !packet.participantIdentity || room?.remoteParticipants.has(packet.participantIdentity)) return;
      receiveData(packet.value.value.payload,packet.participantIdentity,packet.value.value.topic);
    });
    await timeout(room.connect(adapter.toString(), accessToken, { autoSubscribe: false }));
    if (stopped) { await room.disconnect(); return; }
    ready = true;
    publish();
    void pollLocation();
    timer = setInterval(() => {
      for (const [peer, location] of locations) if (!freshLocation(location) || blocked.has(peer)) locations.delete(peer);
      if (ready) publish();
      if (Date.now() < lastPoll || Date.now() - lastPoll >= 15000) void pollLocation();
    }, 5000);
  };
  void start().catch(error => { fail(error); socket?.close(); void room?.disconnect(); });
  return {
    close,
    voice: { call: (peer:string) => voice?.call(peer), accept: () => voice?.accept(), join: (community:string,start=false) => voice?.join(community,start), leave: () => voice?.leave(), mute: () => voice?.mute(), deafen: () => voice?.deafen(), hand: () => voice?.hand(), moderate:(action:VoiceModeration,address?:string)=>voice?.moderate(action,address),localMute:(address:string)=>voice?.localMute(address),device:async(kind:"audioinput"|"audiooutput",id:string)=>{await voice?.device(kind,id);} },
    async settings(value?:SocialSettings){if(!readSettings||!writeSettings||stopped)throw new Error("Connect to Decentraland to load privacy settings.");return value?writeSettings(value):readSettings();},
    async friendship(action: FriendshipAction, address: string) { if (!friendshipAction || stopped) throw new Error("Connect to Decentraland first."); await friendshipAction(action, address); },
    async block(address: string, value: boolean) { if (!blockAction || stopped) throw new Error("Connect to Decentraland first."); await blockAction(address, value); },
    async mutual(address: string) { if (!mutualAction || stopped) throw new Error("Connect to Decentraland first."); return mutualAction(address); },
    async send(peer: string, text: string) {
      peer = peer.toLowerCase();
      const participant = peerParticipant(peer);
      if (!ready || !room || stopped || blocked.has(peer) || !friends.some(f => f.address.toLowerCase() === peer)) throw new Error("Your friend is not available to chat right now.");
      if (!text.trim() || text.length > 4000) throw new Error("Write a message of up to 4,000 characters.");
      // Match Explorer's DateTime.UtcNow.ToOADate(). Unix seconds make its
      // DateTime.FromOADate() throw before the incoming message is displayed.
      const timestamp = Date.now() / 86400000 + 25569;
      const bytes = Packet.encode({ protocolVersion: 100, message: { $case: "chat", chat: { message: text.trim(), timestamp } } }).finish();
      // LiveKit participant discovery can lag behind the authenticated social list.
      // Target the wallet directly; never turn an absent participant into a broadcast.
      try {
        await timeout(room.localParticipant.publishData(new Uint8Array(bytes), { reliable: true, topic: peer, destinationIdentities: [participant?.identity || peer] }));
      } catch (error) {
        fail(new Error(`Outgoing chat failed: ${error instanceof Error ? error.message : 'Data channel unavailable'}`));
        throw new Error('Sending could not be confirmed. Your draft is kept; check the conversation before retrying.');
      }
      receive({ id: crypto.randomUUID(), peer, sender: identity.address.toLowerCase(), text: text.trim(), time: Date.now() });
    },
  };
}


/** Rebuild all subscriptions after transport loss; never replay a user message. */
export function connectFriends(identity: WalletIdentity, update: (state: FriendsState) => void, receive: (message: DirectMessage) => void) {
  let client: ReturnType<typeof openFriends> | undefined;
  let closed = false, generation = 0, attempts = 0;
  let retry: ReturnType<typeof setTimeout> | undefined;
  let latest: FriendsState = {friends:[],ready:false,error:''};
  const start = () => {
    if (closed) return;
    clearTimeout(retry); retry = undefined;
    const current = ++generation;
    client?.close();
    client = openFriends(identity, next => {
      if (closed || current !== generation) return;
      latest = {...next,friends: !next.ready && !next.friends.length ? latest.friends.map(f=>({...f,available:false,location:null,status:undefined})) : next.friends};
      if (next.ready) attempts = 0;
      if (!next.ready && next.error && !next.recovering && next.retryable !== false && identity.canSignSilently && !retry) {
        const expired = identity.expiresAt && identity.expiresAt <= Date.now();
        if (!expired) {
          const delay = Math.min(30000,1000 * 2 ** Math.min(attempts++,5));
          latest.error = navigator.onLine ? 'Reconnecting\u2026' : 'Offline. Waiting for connection\u2026';
          retry = setTimeout(() => { retry=undefined; if(navigator.onLine)start(); },delay);
        }
      }
      update(latest);
    }, message => {if(!closed && current===generation)receive(message);});
  };
  const resume = () => { if (!closed && !latest.ready && latest.error && !latest.recovering && latest.retryable !== false && identity.canSignSilently && navigator.onLine) start(); };
  window.addEventListener('online',resume);
  window.addEventListener('focus',resume);
  start();
  return {
    close() {closed=true;generation++;clearTimeout(retry);window.removeEventListener('online',resume);window.removeEventListener('focus',resume);client?.close();},
    voice: {call:(peer:string)=>client?.voice.call(peer),accept:()=>client?.voice.accept(),join:(community:string,start=false)=>client?.voice.join(community,start),leave:()=>client?.voice.leave(),mute:()=>client?.voice.mute(),deafen:()=>client?.voice.deafen(),hand:()=>client?.voice.hand(),moderate:(action:VoiceModeration,address?:string)=>client?.voice.moderate(action,address),localMute:(address:string)=>client?.voice.localMute(address),device:async(kind:"audioinput"|"audiooutput",id:string)=>{await client?.voice.device(kind,id);}},
    async settings(value?:SocialSettings){if(!client||closed)throw new Error("Connect to Decentraland to load privacy settings.");return client.settings(value);},
    async friendship(action:FriendshipAction,address:string){if(!client||closed||!latest.ready)throw new Error('Reconnecting. Try again in a moment.');await client.friendship(action,address);},
    async block(address:string,value:boolean){if(!client||closed||!latest.ready)throw new Error("Connect to Decentraland first.");await client.block(address,value);},
    async mutual(address:string){if(!client||closed||!latest.ready)throw new Error("Connect to Decentraland first.");return client.mutual(address);},
    async send(peer:string,text:string){if(!client||closed||!latest.ready)throw new Error('Reconnecting. Your message has not been sent.');await client.send(peer,text);},
  };
}
