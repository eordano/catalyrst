import { useCallback, useEffect, useRef, useState } from "react";

import SocialSurface, {
  type SocialListItem,
  type SocialMember,
  type SocialMessage,
  type SocialTab,
} from "@ui/marketplace/social/SocialSurface";

import { useAuth } from "@data/lib/auth/context";
import type { AuthIdentity } from "@data/lib/auth/types";
import { openSignIn } from "../auth/signin-store";
import { fetchMyCommunities, type ProfileCommunity } from "@data/lib/catalyst/overlay/profile-communities";
import {
  friendLabel,
  loadCommunityMembers,
  loadCommunityPosts,
  loadDmThread,
  loadFriends,
  postCommunityMessage,
  relativeTime,
  sendDm,
  shortAddress,
  type CommunityMemberRow,
  type CommunityPost,
  type DirectMessage,
  type Friend,
} from "@data/lib/catalyst/social/social";

type Status = "idle" | "loading" | "ready" | "empty" | "error";

export default function AccountSocial() {
  const auth = useAuth();
  const identity = auth.identity;
  const me = (auth.address ?? "").toLowerCase();
  const [tab, setTab] = useState<SocialTab>("communities");

  if (!identity) {
    return (
      <SocialSurface
        tab={tab}
        onTab={setTab}
        signedOut
        onSignIn={() => openSignIn()}
      />
    );
  }

  return tab === "communities" ? (
    <CommunitiesPane identity={identity} me={me} tab={tab} onTab={setTab} />
  ) : (
    <FriendsPane identity={identity} me={me} tab={tab} onTab={setTab} />
  );
}

function CommunitiesPane({
  identity,
  me,
  tab,
  onTab,
}: {
  identity: AuthIdentity;
  me: string;
  tab: SocialTab;
  onTab: (t: SocialTab) => void;
}) {
  const [comms, setComms] = useState<ProfileCommunity[]>([]);
  const [listStatus, setListStatus] = useState<Status>("loading");
  const [activeId, setActiveId] = useState<string | null>(null);
  const [posts, setPosts] = useState<CommunityPost[]>([]);
  const [members, setMembers] = useState<CommunityMemberRow[]>([]);
  const [threadStatus, setThreadStatus] = useState<Status>("idle");
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setListStatus("loading");
    fetchMyCommunities(identity)
      .then((rows) => {
        if (!alive) return;
        setComms(rows);
        setListStatus(rows.length ? "ready" : "empty");
        if (rows.length && !activeId) setActiveId(rows[0].id);
      })
      .catch(() => alive && setListStatus("error"));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [identity]);

  const refreshPosts = useCallback(
    (id: string) => {
      let alive = true;
      setThreadStatus("loading");
      loadCommunityPosts(id)
        .then(({ posts: p }) => {
          if (!alive) return;
          setPosts(p);
          setThreadStatus(p.length ? "ready" : "empty");
        })
        .catch(() => alive && setThreadStatus("error"));
      return () => {
        alive = false;
      };
    },
    [],
  );

  useEffect(() => {
    if (!activeId) return;
    setDraft("");
    setSendError(null);
    const stop = refreshPosts(activeId);
    let alive = true;
    setMembers([]);
    loadCommunityMembers(activeId, identity)
      .then((rows) => alive && setMembers(rows))
      .catch(() => alive && setMembers([]));
    return () => {
      alive = false;
      stop();
    };
  }, [activeId, refreshPosts, identity]);

  const active = comms.find((c) => c.id === activeId) ?? null;
  const canPost = !!active && (active.role === "owner" || active.role === "admin");

  async function onSend() {
    if (!activeId || !draft.trim()) return;
    setSending(true);
    setSendError(null);
    try {
      const created = await postCommunityMessage(activeId, draft.trim(), identity);
      setPosts((prev) => [...prev, created]);
      setThreadStatus("ready");
      setDraft("");
    } catch (err) {
      const msg = (err as Error)?.message ?? "";
      setSendError(
        /403|permission|forbidden/i.test(msg)
          ? "You need to be a member with posting rights to write here."
          : "Your message couldn't be posted. Please try again.",
      );
    } finally {
      setSending(false);
    }
  }

  const items: SocialListItem[] = comms.map((c) => ({
    id: c.id,
    title: c.name,
    subtitle: memberCountLabel(c.membersCount),
    avatarUrl: thumbUrl(c.thumb),
    badge: c.role === "owner" ? "Owner" : c.role === "admin" ? "Admin" : undefined,
  }));

  const messages: SocialMessage[] = posts.map((p) => ({
    id: p.id,
    author: p.authorName ?? shortAddress(p.authorAddress),
    mine: p.authorAddress.toLowerCase() === me,
    body: p.content,
    time: relativeTime(p.createdAt),
  }));

  const memberChips: SocialMember[] = members.map((m) => ({
    id: m.address,
    name: m.name ?? shortAddress(m.address),
    avatarUrl: m.avatarUrl,
    isFriend: m.isFriend,
    role: m.role,
  }));

  return (
    <SocialSurface
      tab={tab}
      onTab={onTab}
      items={items}
      activeId={activeId}
      onSelect={setActiveId}
      listStatus={listStatus}
      listEmptyText="You haven't joined any communities yet."
      convTitle={active?.name}
      convSubtitle={
        active ? `${memberCountLabel(active.membersCount)} \u{B7} community channel` : undefined
      }
      convAvatarUrl={thumbUrl(active?.thumb ?? null)}
      members={memberChips}
      messages={messages}
      threadStatus={threadStatus}
      threadEmptyText={"No posts in this channel yet \u{2014} start the conversation."}
      showAuthors
      canPost={canPost}
      composerHint={
        active
          ? "Only owners and moderators can post in this channel \u{2014} you can read it here."
          : undefined
      }
      composerPlaceholder={"Post to this channel\u{2026}"}
      draft={draft}
      onDraft={setDraft}
      onSend={onSend}
      sending={sending}
      sendError={sendError}
    />
  );
}

/** "members unknown", never "0 members": the API not reporting a size is not
 *  the same fact as an empty community. */
function memberCountLabel(count: number | null): string {
  if (count === null) return "members unknown";
  return `${count} member${count === 1 ? "" : "s"}`;
}

function thumbUrl(thumb: string | null): string | null {
  if (!thumb) return null;
  const m = thumb.match(/url\("?(.*?)"?\)/);
  return m ? m[1] : null;
}

function FriendsPane({
  identity,
  me,
  tab,
  onTab,
}: {
  identity: AuthIdentity;
  me: string;
  tab: SocialTab;
  onTab: (t: SocialTab) => void;
}) {
  const [friends, setFriends] = useState<Friend[]>([]);
  const [listStatus, setListStatus] = useState<Status>("loading");
  const [activeAddr, setActiveAddr] = useState<string | null>(null);
  const [thread, setThread] = useState<DirectMessage[]>([]);
  const [threadStatus, setThreadStatus] = useState<Status>("idle");
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    let alive = true;
    setListStatus("loading");
    loadFriends(identity)
      .then((rows) => {
        if (!alive) return;
        setFriends(rows);
        setListStatus(rows.length ? "ready" : "empty");
        if (rows.length && !activeAddr) setActiveAddr(rows[0].address);
      })
      .catch(() => alive && setListStatus("error"));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [identity]);

  const refresh = useCallback(
    (addr: string, initial: boolean) => {
      let alive = true;
      loadDmThread(addr, identity)
        .then((msgs) => {
          if (!alive) return;
          setThread(msgs);
          setThreadStatus(msgs.length ? "ready" : "empty");
        })
        .catch(() => {
          if (alive && initial) setThreadStatus("error");
        });
      return () => {
        alive = false;
      };
    },
    [identity],
  );

  useEffect(() => {
    if (!activeAddr) return;
    setDraft("");
    setSendError(null);
    setThreadStatus("loading");
    const stop = refresh(activeAddr, true);
    pollRef.current = setInterval(() => refresh(activeAddr, false), 5000);
    return () => {
      stop();
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [activeAddr, refresh]);

  const active = friends.find((f) => f.address === activeAddr) ?? null;

  async function onSend() {
    if (!activeAddr || !draft.trim()) return;
    setSending(true);
    setSendError(null);
    try {
      const msg = await sendDm(activeAddr, draft.trim(), identity);
      setThread((prev) => [...prev, msg]);
      setThreadStatus("ready");
      setDraft("");
    } catch (err) {
      const m = (err as Error)?.message ?? "";
      setSendError(
        /403|forbidden|friend/i.test(m)
          ? "You can only message accepted friends."
          : "Your message couldn't be sent. Please try again.",
      );
    } finally {
      setSending(false);
    }
  }

  const items: SocialListItem[] = friends.map((f) => ({
    id: f.address,
    title: friendLabel(f),
    subtitle: f.name ? shortAddress(f.address) : undefined,
    avatarUrl: f.avatarUrl,
  }));

  const messages: SocialMessage[] = thread.map((m) => ({
    id: m.id,
    author: m.from.toLowerCase() === me ? "You" : active ? friendLabel(active) : shortAddress(m.from),
    mine: m.from.toLowerCase() === me,
    body: m.body,
    time: relativeTime(m.sentAt),
  }));

  return (
    <SocialSurface
      tab={tab}
      onTab={onTab}
      items={items}
      activeId={activeAddr}
      onSelect={setActiveAddr}
      listStatus={listStatus}
      listEmptyText="No friends yet. Add friends in-world and they'll show up here."
      convTitle={active ? friendLabel(active) : undefined}
      convSubtitle={active?.name ? shortAddress(active.address) : "Direct message"}
      convAvatarUrl={active?.avatarUrl ?? null}
      messages={messages}
      threadStatus={threadStatus}
      threadEmptyText={"No messages yet \u{2014} say hello."}
      canPost={!!active}
      composerPlaceholder={"Message your friend\u{2026}"}
      draft={draft}
      onDraft={setDraft}
      onSend={onSend}
      sending={sending}
      sendError={sendError}
    />
  );
}
