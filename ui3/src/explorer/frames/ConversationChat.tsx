import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { loadConversation, loadConversations, sendConversationMessage, type Conversation, type ConversationKind } from "../../data/catalyst/conversations";
import { useBridgeState } from "../../overlay/bridge";
import { truncateAddress } from "../../data/format";
import { Avatar } from "../../atoms/primitives";
import { ChatView, type ChatIo } from "./Chat";
import ProfileCard from "../components/ProfileCard";
import "./conversationchat.css";

export default function ConversationChat({ kind, active = true }: { kind: ConversationKind; active?: boolean }) {
  const identity = useBridgeState((s) => s.identity);
  const signedIn = !!identity.address && !identity.isGuest;
  const [selected, setSelected] = useState("");
  const [search, setSearch] = useState("");
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const list = useQuery({ queryKey: ["chat-conversations", kind, identity.address], queryFn: () => loadConversations(kind), enabled: signedIn && active, refetchInterval: active ? 30000 : false });
  const conversation = list.data?.find((c) => c.id === selected);
  if (!signedIn) return <p className="cchat__notice">Sign in with a wallet to use {kind === "direct" ? "direct messages" : "community chat"}.</p>;
  const showSearch = kind === "direct" || (list.data?.length ?? 0) > 5;
  const matches = list.data?.filter((c) => `${c.name} ${c.id}`.toLowerCase().includes(showSearch ? search.toLowerCase() : ""));
  return <div className="cchat">
    {conversation && identity.address ? <>
      <header className="cchat__heading">
        <button type="button" className="cchat__back" aria-label={kind === "direct" ? "Back to direct messages" : "Back to communities"} onClick={() => setSelected("")}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="m14 6-6 6 6 6" /></svg>
        </button>
        <Avatar size={32} name={conversation.name} seed={conversation.id} />
        <div><strong>{label(conversation)}</strong><small>{kind === "direct" ? "Direct message" : "Community"}</small></div>
      </header>
      <Thread key={`${kind}:${conversation.id}`} kind={kind} conversation={conversation} address={identity.address} name={identity.name} active={active} draft={drafts[conversation.id] ?? ""} onDraftChange={(draft) => setDrafts((prev) => ({ ...prev, [conversation.id]: draft }))} />
    </> : <>
      {showSearch && <input type="search" className="cchat__search" aria-label={kind === "direct" ? "Search friends" : "Search communities"} placeholder={kind === "direct" ? "Find a friend to message" : "Find a community"} value={search} onChange={(event) => setSearch(event.target.value)} />}
      {list.isPending && <p className="cchat__notice" role="status">Loading {kind === "direct" ? "friends" : "communities"}&#x2026;</p>}
      {list.isError && <p role="alert">Could not load conversations. <button onClick={() => { void list.refetch(); }}>Retry</button></p>}
      {!list.isPending && !list.isError && !list.data?.length && <p className="cchat__notice">{kind === "direct" ? "Add a friend to start a direct conversation." : "Join a community to talk with its members."}</p>}
      {!!list.data?.length && !matches?.length && <p className="cchat__notice">No matches. Try another name.</p>}
      <ul className="cchat__conversations">
        {matches?.map((conversation) => <li key={conversation.id}><button type="button" onClick={() => setSelected(conversation.id)}>
          <Avatar size={38} name={conversation.name} seed={conversation.id} />
          <span><strong>{label(conversation)}</strong><small>{drafts[conversation.id] ? "Draft saved" : kind === "direct" ? "Direct message" : "Community"}</small></span>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="m9 6 6 6-6 6" /></svg>
        </button></li>)}
      </ul>
    </>}
  </div>;
}

function label(conversation: Conversation): string {
  return /^0x[0-9a-f]{40}$/i.test(conversation.name) ? truncateAddress(conversation.name) : conversation.name;
}

function Thread({ kind, conversation, address, name, active, draft, onDraftChange }: {
  kind: ConversationKind; conversation: Conversation; address: string; name: string; active: boolean; draft: string; onDraftChange: (draft: string) => void;
}) {
  const thread = useQuery({ queryKey: ["chat-thread", kind, conversation.id, address], queryFn: () => loadConversation(kind, conversation.id), enabled: active, refetchInterval: active ? 5000 : false });
  const io = useMemo<ChatIo>(() => ({
    chat: (thread.data ?? []).map((message) => ({ senderAddress: message.author, senderName: message.author.toLowerCase() === address.toLowerCase() ? "You" : kind === "direct" ? label(conversation) : message.author, message: message.body, timestamp: Date.parse(message.date) || 0, channel: kind })),
    me: { address, name }, players: [], blocked: [], live: !thread.isPending,
    send: async (message) => {
      await sendConversationMessage(kind, conversation.id, message);
      await thread.refetch();
    },
  }), [thread.data, thread.isPending, thread.refetch, kind, conversation.id, conversation.name, address, name]);
  return <>
    {thread.isError && <p className="cchat__notice" role="alert">Could not refresh messages. <button onClick={() => { void thread.refetch(); }}>Retry</button></p>}
    <ChatView open={active} onToggle={() => {}} io={io} docked header={false} title={label(conversation)} commands={false} draftValue={draft} onDraftChange={onDraftChange} profileCard={ProfileCard} />
  </>;
}
