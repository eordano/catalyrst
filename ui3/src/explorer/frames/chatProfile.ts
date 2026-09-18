import { createContext, useContext } from "react";

export type ViewChatProfile = (address: string, opener?: Element | null) => void;

export const ChatProfileContext = createContext<ViewChatProfile | null>(null);

export const useChatProfile = (): ViewChatProfile | null => useContext(ChatProfileContext);

const FROM_CHAT = "chat";

export const CHAT_PROFILE_STATE = { from: FROM_CHAT };

export function chatProfilePath(address: string): string {
  return `/passport?address=${encodeURIComponent(address)}`;
}

export function isChatProfile(location: { pathname: string; state: unknown }): boolean {
  const from = (location.state as { from?: unknown } | null)?.from;
  return from === FROM_CHAT && location.pathname.replace(/^\/+/, "").split("/")[0] === "passport";
}
