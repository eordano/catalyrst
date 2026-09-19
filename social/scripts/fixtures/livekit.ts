export const EngineEvent={DataPacketReceived:"packet"};
export const DisconnectReason={DUPLICATE_IDENTITY:2,PARTICIPANT_REMOVED:4};
// Test-only transport. The test build aliases livekit-client to this module.
export const Track = {Kind:{Audio:"audio"}};
export const RoomEvent = { ParticipantMetadataChanged:"metadata",ParticipantAttributesChanged:"attributes",TrackMuted:"muted",TrackUnmuted:"unmuted",TrackSubscribed:"track",TrackUnsubscribed:"untrack",ActiveSpeakersChanged:"speakers",ParticipantPermissionsChanged:"permissions",LocalTrackPublished:"localtrack",LocalTrackUnpublished:"localuntrack", ParticipantConnected: "joined", ParticipantDisconnected: "left", Reconnecting: "reconnecting", Reconnected: "reconnected", Disconnected: "disconnected", DataReceived: "data" };
export class Room {
  state = "disconnected";
  remoteParticipants = new Map([["peer", { identity: "0x0000000000000000000000000000000000000002",metadata:JSON.stringify({role:"member",isRequestingToSpeak:true}),permissions:{canPublish:false},isMicrophoneEnabled:false }]]);
  engine = {on: (event:string,callback:Function)=>this.on(event,callback)};
  handlers = new Map<string, Function[]>();
  on(event: string, callback: Function) { this.handlers.set(event, [...(this.handlers.get(event) || []), callback]); }
  emit(event: string, ...args: unknown[]) { for (const callback of this.handlers.get(event) || []) callback(...args); }
  localParticipant = { identity:"0x0000000000000000000000000000000000000001", metadata:JSON.stringify({role:"owner"}), isSpeaking:false, permissions:{canPublish:true}, isMicrophoneEnabled:false, setMicrophoneEnabled:async (value:boolean) => { this.localParticipant.isMicrophoneEnabled = value; }, publishData: async (bytes: Uint8Array, options: unknown) => {
    (window as any).published.push({ bytes: Array.from(bytes), options });
  } };
  async switchActiveDevice(kind:string,id:string) { if((window as any).failDeviceSwitch)throw new Error('Test device unavailable'); (window as any).selectedAudio={kind,id};return true; }
  async startAudio() {}
  async connect() { this.state = "connected"; (window as any).room = this; (window as any).published = []; }
  async disconnect() { this.state = "disconnected"; this.emit("disconnected"); }
}
