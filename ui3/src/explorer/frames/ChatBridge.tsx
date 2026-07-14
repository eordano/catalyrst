import { useMemo } from "react";

import { sendBridge, useBridgeState } from "../../overlay/bridge";
import ProfileCard from "../components/ProfileCard";
import { ChatView, type ChatIo } from "./Chat";

// The explorer's chat: ChatView wired to the overlay bridge. Split from
// Chat.tsx so hosts that bring their own transport (their own page rooms)
// can import the view without pulling the bridge -- and its validate/zod
// dependency chain -- into their bundle.
export default function Chat(props: {
  open: boolean;
  onToggle: () => void;
  hidden?: boolean;
}) {
  const chat = useBridgeState((s) => s.chat);
  const players = useBridgeState((s) => s.players);
  const identity = useBridgeState((s) => s.identity);
  const blocked = useBridgeState((s) => s.friends.blocked);
  const live = useBridgeState((s) => s.live);
  const io = useMemo<ChatIo>(
    () => ({
      chat,
      players,
      blocked,
      live,
      me: identity.address ? { address: identity.address, name: identity.name } : null,
      send: (message) => sendBridge("SendChat", { channel: "Nearby", message }),
      teleport: (x, z) => sendBridge("Teleport", { x, z }),
      changeRealm: (realm) => sendBridge("ChangeRealm", { realm }),
    }),
    [chat, players, blocked, live, identity.address, identity.name],
  );
  return <ChatView {...props} io={io} profileCard={ProfileCard} />;
}
