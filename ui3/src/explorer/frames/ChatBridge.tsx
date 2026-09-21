import { useEffect, useMemo, useState } from "react";
import FloatingPanel from "../components/FloatingPanel";
import ConversationChat from "./ConversationChat";

import { sendBridge, subscribeBridge, useBridgeState } from "../../overlay/bridge";
import ProfileCard from "../components/ProfileCard";
import { ChatView, type ChatIo } from "./Chat";
import type { ConsoleLine, ConsoleSource } from "./chatCommands";
import { useChatProfile } from "./chatProfile";
import { useChatIntent } from "./chatIntent";

function isChatPush(push: unknown): push is ConsoleLine & { kind: "chat" } {
  return typeof push === "object" && push !== null && (push as { kind?: unknown }).kind === "chat";
}

const consoleSource: ConsoleSource = (listener) =>
  subscribeBridge((push) => {
    if (isChatPush(push)) listener(push);
  });

export default function Chat(props: {
  open: boolean;
  onToggle: () => void;
  hidden?: boolean;
}) {
  const [channel, setChannel] = useState<"nearby" | "direct" | "community">("nearby");
  const chat = useBridgeState((s) => s.chat);
  const players = useBridgeState((s) => s.players);
  const identity = useBridgeState((s) => s.identity);
  const blocked = useBridgeState((s) => s.friends.blocked);
  const hasFriends = useBridgeState((s) => s.friends.friends.length > 0);
  const live = useBridgeState((s) => s.live);
  const viewProfile = useChatProfile();
  const intent = useChatIntent();
  useEffect(() => {
    if (intent?.kind === "direct" && hasFriends) setChannel("direct");
  }, [intent, hasFriends]);
  useEffect(() => {
    if (!hasFriends && channel === "direct") setChannel("nearby");
  }, [hasFriends, channel]);
  const io = useMemo<ChatIo>(
    () => ({
      chat,
      players,
      blocked,
      live,
      me: identity.address ? { address: identity.address, name: identity.name } : null,
      send: (message) => sendBridge("SendChat", { channel: "Nearby", message }),
      console: live ? consoleSource : undefined,
      teleport: (x, z) => sendBridge("Teleport", { x, z }),
      changeRealm: (realm) => sendBridge("ChangeRealm", { realm }),
    }),
    [chat, players, blocked, live, identity.address, identity.name],
  );
  return <div hidden={!props.open || props.hidden}>
    <FloatingPanel id="chat" onClose={props.onToggle} closeLabel="Close chat" flush anchorEnabled={false}>
      <div className="chat-channels" role="group" aria-label="Chat channels">
        {(["nearby", "direct", "community"] as const).filter(id => id !== "direct" || hasFriends).map((id) => <button key={id} type="button" aria-pressed={channel === id} onClick={() => setChannel(id)}>{id === "nearby" ? "Nearby" : id === "direct" ? "Friends" : "Communities"}</button>)}
      </div>
      <ChatView {...props} hidden={props.hidden || channel !== "nearby"} io={io} emptyLine="Say hello to people nearby!" profileCard={ProfileCard} onViewProfile={viewProfile ? (user, opener) => viewProfile(user.address, opener) : undefined} docked header={false} />
      {(["direct", "community"] as const).map((kind) => <div key={`${identity.address}:${identity.isGuest}:${kind}`} className="chat-channel" hidden={channel !== kind}>
        <ConversationChat kind={kind} active={props.open && !props.hidden && channel === kind} target={intent?.kind === kind ? intent : null} />
      </div>)}
    </FloatingPanel>
  </div>;
}
