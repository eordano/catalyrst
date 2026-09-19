import { audioPreferences, saveAudioPreferences, type AudioPreferences } from './audioPreferences';
import { Room, RoomEvent, Track } from 'livekit-client';
import type { RpcClientModule } from '@dcl/rpc/dist/codegen';
import { SocialServiceDefinition, PrivateVoiceChatStatus as Status, type PrivateVoiceChatUpdate } from '@dcl/protocol/out-js/decentraland/social_service/v2/social_service_v2.gen';
export type VoiceModeration = "promote" | "demote" | "reject" | "mute" | "unmute" | "kick" | "end";
export type VoiceState = { host: boolean; hostMuted: boolean; moderating: boolean; community: string; peer: string; callId: string; incoming: boolean; ringing: boolean; connected: boolean; busy: boolean; muted: boolean; deafened: boolean; canSpeak: boolean; raised: boolean; participants: {wallet: string; speaking: boolean; speaker: boolean; raised: boolean; muted: boolean; hostMuted: boolean; localMuted: boolean; role: string}[]; error: string };
export const emptyVoice: VoiceState = {host:false,hostMuted:false,moderating:false,community:'',peer:'',callId:'',incoming:false,ringing:false,connected:false,busy:false,muted:true,deafened:false,canSpeak:false,raised:false,participants:[],error:''};
export function createVoice(service: RpcClientModule<typeof SocialServiceDefinition>, update: (state: VoiceState) => void, self: string, allowed: (peer: string) => boolean) {
  let state = {...emptyVoice};
  let room: Room | undefined;
  let version = 0;
  let closed = false;
  let preferences = audioPreferences();
  let connectingCall = '';
  let ringTimer: ReturnType<typeof setTimeout> | undefined;
  const audio = new Set<HTMLMediaElement>();
  const audioPeers = new Map<HTMLMediaElement,string>();
  const mutedPeers = new Set<string>();
  const publish = (patch: Partial<VoiceState> = {}) => { state = {...state,...patch}; if (!closed) update(state); };
  const timeout = <T,>(promise: Promise<T>) => new Promise<T>((resolve,reject) => { const timer = setTimeout(() => reject(new Error('Voice request timed out. Refresh before trying again.')),15000); promise.then(value => {clearTimeout(timer);resolve(value);},e => {clearTimeout(timer);reject(e);}); });
  const error = (e: unknown) => publish({busy:false,error:e instanceof Error ? e.message : 'Voice connection failed.'});
  const preferenceChange = (event: Event) => { preferences = (event as CustomEvent<AudioPreferences>).detail; audio.forEach(el => {el.volume=preferences.volume/100;}); if (!preferences.incomingCalls && state.incoming && state.ringing) void leave(); };
  window.addEventListener('social:audio-preferences',preferenceChange);
  const devicesChanged=async()=>{const current=room;if(!current||!state.connected)return;try{const devices=await navigator.mediaDevices.enumerateDevices();if(room!==current)return;for(const kind of ['audioinput','audiooutput'] as const){const key=kind==='audioinput'?'input':'output';if(preferences[key]!=='default'&&!devices.some(d=>d.kind===kind&&d.deviceId===preferences[key])){if(!await current.switchActiveDevice(kind,'default'))throw new Error('Choose an available audio device in Audio settings.');saveAudioPreferences({[key]:'default'});publish({error:'An audio device disconnected. Using the system default.'});}}}catch(e){if(room===current)error(e);}};
  navigator.mediaDevices?.addEventListener('devicechange',devicesChanged);
  const detach = () => { audio.forEach(el => { el.pause(); el.remove(); }); audio.clear();audioPeers.clear();mutedPeers.clear(); };
  async function reset(message = '') {
    version++; clearTimeout(ringTimer); connectingCall = ''; const old = room; room = undefined; detach(); publish({...emptyVoice,error:message}); await old?.disconnect();
  }
  async function leave() {
    const callId = state.callId, incoming = state.incoming && state.ringing;
    await reset();
    if (callId) {
      try { const r = await timeout(incoming ? service.rejectPrivateVoiceChat({callId}) : service.endPrivateVoiceChat({callId})); if (r.response?.$case !== 'ok' && r.response?.$case !== 'notFound') throw new Error('Audio stopped. The call could not be closed upstream.'); } catch(e) { if (!closed) error(e); }
    }
  }
  async function connect(raw: string | undefined, current: number) {
    if (!raw) throw new Error('Voice credentials were not provided.');
    const url = new URL(raw.replace(/^livekit:/,''));
    if (url.protocol !== 'wss:' || url.username || url.password) throw new Error('Invalid voice server.');
    const token = url.searchParams.get('access_token');
    if (!token) throw new Error('Voice token was not provided.');
    url.searchParams.delete('access_token');
    const next = new Room(); room = next;
    const metadata = (raw: string | undefined) => { try { const value=JSON.parse(raw || '{}');return value&&typeof value==='object'&&!Array.isArray(value)?value:{}; } catch { return {}; } };
    const refresh = () => { if (room !== next) return;
      const own = metadata(next.localParticipant.metadata);
      const canSpeak = next.localParticipant.permissions?.canPublish === true;
      if ((own.muted === true || !canSpeak) && next.localParticipant.isMicrophoneEnabled) void next.localParticipant.setMicrophoneEnabled(false).catch(error);
      publish({host:own.role === 'owner' || own.role === 'moderator',hostMuted:own.muted === true,raised:own.isRequestingToSpeak === true,participants:[next.localParticipant,...next.remoteParticipants.values()].filter(p => /^0x[0-9a-f]{40}$/i.test(p.identity)).map(p => { const info = metadata(p.metadata); return {wallet:p.identity,speaking:p.isSpeaking,speaker:p.permissions?.canPublish === true || info.isSpeaker === true,raised:info.isRequestingToSpeak === true,muted:info.muted === true || !p.isMicrophoneEnabled,hostMuted:info.muted === true,localMuted:mutedPeers.has(p.identity.toLowerCase()),role:info.role || 'member'}; }),canSpeak:next.localParticipant.permissions?.canPublish === true,muted:!next.localParticipant.isMicrophoneEnabled}); };
    next.on(RoomEvent.TrackSubscribed,(track,_publication,participant) => { if (track.kind !== Track.Kind.Audio || room !== next) return; const el = track.attach(); el.hidden = true; el.muted = state.deafened || mutedPeers.has(participant.identity.toLowerCase()); audioPeers.set(el,participant.identity.toLowerCase()); el.volume = preferences.volume/100; audio.add(el); document.body.append(el); });
    next.on(RoomEvent.TrackUnsubscribed,track => { track.detach().forEach(el => { audio.delete(el); audioPeers.delete(el); el.remove(); }); });
    for (const event of [RoomEvent.ParticipantConnected,RoomEvent.ParticipantDisconnected,RoomEvent.ActiveSpeakersChanged,RoomEvent.ParticipantPermissionsChanged,RoomEvent.ParticipantMetadataChanged,RoomEvent.ParticipantAttributesChanged,RoomEvent.TrackMuted,RoomEvent.TrackUnmuted,RoomEvent.LocalTrackPublished,RoomEvent.LocalTrackUnpublished] as const) next.on(event,refresh);
    next.on(RoomEvent.Disconnected,() => { if (room === next) void leave(); });
    await next.connect(url.toString(),token);
    if (closed || current !== version) { await next.disconnect(); return; }
    for (const kind of ['audioinput','audiooutput'] as const) {
      const key=kind==='audioinput'?'input':'output';
      if(preferences[key]==='default')continue;
      try { if(!await next.switchActiveDevice(kind,preferences[key]))throw new Error('Device unavailable'); }
      catch { await next.switchActiveDevice(kind,'default');saveAudioPreferences({[key]:'default'}); }
    }
    await next.startAudio(); clearTimeout(ringTimer);
    publish({connected:true,busy:false,ringing:false,error:''}); refresh();
  }
  async function failed(e: unknown, current: number) {
    if (current !== version || closed) return;
    await leave(); error(e);
  }
  function ringing() { clearTimeout(ringTimer); ringTimer = setTimeout(() => { void leave(); },60000); }
  async function event(event: PrivateVoiceChatUpdate) {
    const caller = event.caller?.address.toLowerCase(), callee = event.callee?.address.toLowerCase();
    if (event.status === Status.VOICE_CHAT_REQUESTED) {
      if (callee !== self.toLowerCase() || !caller || !allowed(caller) || state.callId || state.community || state.busy) return;
      if (!preferences.incomingCalls) { await timeout(service.rejectPrivateVoiceChat({callId:event.callId})).catch(()=>{}); return; }
      publish({...emptyVoice,peer:caller,callId:event.callId,incoming:true,ringing:true}); ringing(); return;
    }
    if (!state.callId && state.busy && state.peer && caller === self.toLowerCase() && callee === state.peer) publish({callId:event.callId});
    if (!state.callId || event.callId !== state.callId) return;
    if (event.status === Status.VOICE_CHAT_ACCEPTED && !state.incoming && event.credentials && connectingCall !== event.callId && !state.connected) {
      if (!allowed(state.peer)) { await leave(); return; }
      connectingCall = event.callId; const current = version; publish({busy:true,ringing:false});
      try { await connect(event.credentials.connectionUrl,current); } catch(e) { await failed(e,current); }
    } else if ([Status.VOICE_CHAT_REJECTED,Status.VOICE_CHAT_ENDED,Status.VOICE_CHAT_EXPIRED].includes(event.status)) {
      await reset(event.status === Status.VOICE_CHAT_REJECTED ? 'Call declined.' : event.status === Status.VOICE_CHAT_EXPIRED ? 'No answer.' : '');
    }
  }
  // Incoming requests and call termination use the same authenticated RPC stream as friends.
  void (async () => { try { for await (const value of service.subscribeToPrivateVoiceChatUpdates({})) { if (closed) return; await event(value); } if (!closed && state.callId) await reset('Call signaling disconnected. Reconnect to call again.'); } catch { if (!closed && state.callId) { await reset('Call signaling disconnected. Reconnect to call again.'); } } })();
  return {
    async syncIncoming() { try { const r = await timeout(service.getIncomingPrivateVoiceChatRequest({})); if (!closed && r.response?.$case === 'ok') await event({callId:r.response.ok.callId,caller:r.response.ok.caller,callee:{address:self},status:Status.VOICE_CHAT_REQUESTED}); } catch { /* The live stream remains authoritative. */ } },
    async call(peer: string) {
      peer = peer.toLowerCase();
      if (state.busy || state.callId || state.connected) { error(new Error('Leave your current voice chat before starting another call.')); return; }
      if (!allowed(peer)) { error(new Error('Only accepted, unblocked friends can be called.')); return; }
      await reset(); const current = version; publish({peer,busy:true,ringing:true});
      try { const r = await timeout(service.startPrivateVoiceChat({callee:{address:peer}})); if (closed || current !== version) { if (r.response?.$case === 'ok') await service.endPrivateVoiceChat({callId:r.response.ok.callId}); return; } if (r.response?.$case !== 'ok') throw new Error('Your friend could not be called. They may be offline or busy.'); publish({callId:r.response.ok.callId,busy:false}); if (!state.connected) ringing(); } catch(e) { await failed(e,current); }
    },
    async accept() {
      if (!state.incoming || !state.ringing || state.busy || !allowed(state.peer)) return;
      const callId = state.callId, current = version; publish({busy:true});
      try { const r = await timeout(service.acceptPrivateVoiceChat({callId})); if (closed || current !== version) return; if (r.response?.$case !== 'ok') throw new Error('This call is no longer available.'); connectingCall = callId; await connect(r.response.ok.credentials?.connectionUrl,current); } catch(e) { await failed(e,current); }
    },
    async join(community: string, start = false) {
      if (state.busy || state.callId) { error(new Error('End your call before joining a community voice chat.')); return; }
      await leave(); const current = version; publish({...emptyVoice,community,busy:true});
      try {
        const response = start ? await timeout(service.startCommunityVoiceChat({communityId:community})) : await timeout(service.joinCommunityVoiceChat({communityId:community}));
        if (closed || current !== version) return;
        if (response.response?.$case !== 'ok') throw new Error(response.response?.$case === 'notFoundError' ? 'The lounge is quiet. A moderator can start a voice chat.' : 'Could not join voice. Check your community access and try again.');
        await connect(response.response.ok.credentials?.connectionUrl,current);
      } catch(e) { await failed(e,current); }
    },
    leave,
    enforceAccess() { if (state.peer && !allowed(state.peer)) void leave(); },
    async mute() { if (!room || !state.canSpeak) return; if(state.hostMuted){error(new Error("A host has muted your microphone."));return;} try { await room.localParticipant.setMicrophoneEnabled(state.muted); publish({muted:!room.localParticipant.isMicrophoneEnabled,error:''}); } catch(e) { error(e); } },
    deafen() { const deafened = !state.deafened; audio.forEach(el => { el.muted = deafened || mutedPeers.has(audioPeers.get(el)||''); }); publish({deafened}); },
    async hand() { if (!state.connected || !state.community) return; try { const r = await timeout(service.requestToSpeakInCommunityVoiceChat({communityId:state.community,isRaisingHand:!state.raised})); if (r.response?.$case !== 'ok') throw new Error('Could not update your request to speak.'); publish({raised:!state.raised,error:''}); } catch(e) { error(e); } },
    localMute(address:string) { const peer=address.toLowerCase();if(mutedPeers.has(peer))mutedPeers.delete(peer);else mutedPeers.add(peer);audio.forEach(el=>{if(audioPeers.get(el)===peer)el.muted=state.deafened||mutedPeers.has(peer);});publish({participants:state.participants.map(p=>({...p,localMuted:mutedPeers.has(p.wallet.toLowerCase())}))}); },
    async device(kind:'audioinput'|'audiooutput',id:string) { if(room && !await room.switchActiveDevice(kind,id)) throw new Error('This audio device could not be selected.'); },
    async moderate(action:VoiceModeration,userAddress = '') {
      if(!state.connected || !state.community || !state.host || state.moderating)return;
      if(action!=='end'&&!/^0x[0-9a-f]{40}$/i.test(userAddress)){error(new Error('Choose a voice participant.'));return;}
      const current=version, communityId=state.community, target={communityId,userAddress};publish({moderating:true,error:''});
      try {
        const response=await timeout<{response?:{$case:string}}>(action==='promote'?service.promoteSpeakerInCommunityVoiceChat(target):action==='demote'?service.demoteSpeakerInCommunityVoiceChat(target):action==='reject'?service.rejectSpeakRequestInCommunityVoiceChat(target):action==='kick'?service.kickPlayerFromCommunityVoiceChat(target):action==='end'?service.endCommunityVoiceChat({communityId}):service.muteSpeakerFromCommunityVoiceChat({...target,muted:action==='mute'}));
        if(current!==version||closed)return;
        if(response.response?.$case!=='ok')throw new Error(response.response?.$case==='forbiddenError'?'Only community hosts can make this change.':'The voice change could not be completed.');
        if(action==='end'){await reset();return;}
        // The media server's participant metadata and permission events remain authoritative.
        publish({moderating:false});
      }catch(e){if(current===version){publish({moderating:false});error(e);}}
    },
    close() { closed = true; window.removeEventListener('social:audio-preferences',preferenceChange); navigator.mediaDevices?.removeEventListener('devicechange',devicesChanged); void leave(); },
  };
}
