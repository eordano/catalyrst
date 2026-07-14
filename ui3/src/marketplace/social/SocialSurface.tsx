import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import "./social.css";
import { hueFor } from "../../data/format";

export type SocialTab = "communities" | "friends";

export type SocialListItem = {
  id: string;
  title: string;
  subtitle?: string;
  avatarUrl?: string | null;
  badge?: string;
};

export type SocialMessage = {
  id: string;
  author: string;
  mine: boolean;
  body: string;
  time: string;
  avatarUrl?: string | null;
};

export type SocialMember = {
  id: string;
  name: string;
  avatarUrl?: string | null;
  isFriend?: boolean;
  role?: string;
};

type Status = "idle" | "loading" | "ready" | "empty" | "error";

export type SocialSurfaceProps = {
  tab?: SocialTab;
  onTab?: (t: SocialTab) => void;

  items?: SocialListItem[];
  activeId?: string | null;
  onSelect?: (id: string) => void;
  listStatus?: Status;
  listEmptyText?: ReactNode;
  listErrorText?: ReactNode;

  convTitle?: string;
  convSubtitle?: string;
  convAvatarUrl?: string | null;
  members?: SocialMember[];
  messages?: SocialMessage[];
  threadStatus?: Status;
  threadEmptyText?: ReactNode;
  threadErrorText?: ReactNode;

  showAuthors?: boolean;
  canPost?: boolean;
  composerPlaceholder?: string;
  composerHint?: ReactNode;
  draft?: string;
  onDraft?: (s: string) => void;
  onSend?: () => void;
  sending?: boolean;
  sendError?: string | null;

  signedOut?: boolean;
  onSignIn?: () => void;
};


function meaningfulSeed(seed: string): string {
  const s = seed.trim();
  if (/^0x./i.test(s)) return s.slice(2);
  return s;
}

function Avatar({ url, seed, size = 36 }: { url?: string | null; seed: string; size?: number }) {
  const body = meaningfulSeed(seed);
  const initial = (body[0] ?? "?").toUpperCase();
  if (url) {
    return (
      <span
        className="mksocial__avatar"
        style={{ width: size, height: size, backgroundImage: `url("${url.replace(/"/g, "%22")}")` }}
        aria-hidden
      />
    );
  }
  return (
    <span
      className="mksocial__avatar mksocial__avatar--fallback"
      style={{ width: size, height: size, background: `hsl(${hueFor(body)} 55% 32%)` }}
      aria-hidden
    >
      {initial}
    </span>
  );
}

export default function SocialSurface({
  tab = "communities",
  onTab,
  items = [],
  activeId = null,
  onSelect,
  listStatus = "ready",
  listEmptyText = "Nothing here yet.",
  listErrorText = "Couldn't load this list.",
  convTitle,
  convSubtitle,
  convAvatarUrl = null,
  members,
  messages = [],
  threadStatus = "idle",
  threadEmptyText = "No messages yet \u{2014} say hello.",
  threadErrorText = "Couldn't load this conversation.",
  showAuthors = false,
  canPost = false,
  composerPlaceholder = "Write a message\u{2026}",
  composerHint,
  draft = "",
  onDraft,
  onSend,
  sending = false,
  sendError = null,
  signedOut = false,
  onSignIn,
}: SocialSurfaceProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages.length, activeId]);

  if (signedOut) {
    return (
      <div className="mksocial" data-signedout>
        <SocialTabs tab={tab} onTab={onTab} />
        <div className="mksocial__gate" role="status">
          <p className="mksocial__gatetitle">Sign in to chat</p>
          <p className="mksocial__gatebody">
            Message your friends and community channels right here &#x2014; no need to enter the 3D world.
          </p>
          <button type="button" className="mksocial__signin" onClick={onSignIn}>
            Sign in
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="mksocial">
      <SocialTabs tab={tab} onTab={onTab} />
      <div className="mksocial__body">
        <aside className="mksocial__list" aria-label={tab === "friends" ? "Friends" : "Communities"}>
          {listStatus === "loading" ? (
            <p className="mksocial__hint">Loading&#x2026;</p>
          ) : listStatus === "error" ? (
            <p className="mksocial__hint">{listErrorText}</p>
          ) : items.length === 0 ? (
            <p className="mksocial__hint">{listEmptyText}</p>
          ) : (
            <ul className="mksocial__items">
              {items.map((it) => (
                <li key={it.id}>
                  <button
                    type="button"
                    className={"mksocial__item" + (it.id === activeId ? " is-active" : "")}
                    aria-current={it.id === activeId ? "true" : undefined}
                    onClick={() => onSelect?.(it.id)}
                  >
                    <Avatar url={it.avatarUrl} seed={it.title} />
                    <span className="mksocial__itemtext">
                      <span className="mksocial__itemtitle">{it.title}</span>
                      {it.subtitle ? (
                        <span className="mksocial__itemsub">{it.subtitle}</span>
                      ) : null}
                    </span>
                    {it.badge ? <span className="mksocial__badge">{it.badge}</span> : null}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </aside>

        <section className="mksocial__conv" aria-label="Conversation">
          {!activeId ? (
            <div className="mksocial__empty" role="status">
              {tab === "friends"
                ? "Pick a friend to start chatting."
                : "Pick a community channel to read and post."}
            </div>
          ) : (
            <>
              <header className="mksocial__convhead">
                <Avatar url={convAvatarUrl} seed={convTitle ?? "?"} size={32} />
                <span className="mksocial__convtext">
                  <span className="mksocial__convtitle">{convTitle}</span>
                  {convSubtitle ? (
                    <span className="mksocial__convsub">{convSubtitle}</span>
                  ) : null}
                </span>
              </header>

              {members && members.length ? (
                <div className="mksocial__members" aria-label="Channel members">
                  <span className="mksocial__memberslabel">
                    Members ({members.length})
                  </span>
                  {members.map((mem) => (
                    <span
                      key={mem.id}
                      className={"mksocial__member" + (mem.isFriend ? " is-friend" : "")}
                    >
                      <Avatar url={mem.avatarUrl} seed={mem.name} size={22} />
                      <span className="mksocial__membername">{mem.name}</span>
                      {mem.isFriend ? (
                        <span className="mksocial__friendbadge">Friend</span>
                      ) : null}
                    </span>
                  ))}
                </div>
              ) : null}

              <div className="mksocial__messages" ref={scrollRef}>
                {threadStatus === "loading" ? (
                  <p className="mksocial__hint">Loading messages&#x2026;</p>
                ) : threadStatus === "error" ? (
                  <p className="mksocial__hint">{threadErrorText}</p>
                ) : messages.length === 0 ? (
                  <p className="mksocial__hint">{threadEmptyText}</p>
                ) : (
                  messages.map((m) => (
                    <div
                      key={m.id}
                      className={"mksocial__msg" + (m.mine ? " is-mine" : "")}
                    >
                      {!m.mine && showAuthors ? (
                        <Avatar url={m.avatarUrl} seed={m.author} size={28} />
                      ) : null}
                      <div className="mksocial__bubblewrap">
                        {!m.mine && showAuthors ? (
                          <span className="mksocial__msgauthor">{m.author}</span>
                        ) : null}
                        <div className="mksocial__bubble">{m.body}</div>
                        <span className="mksocial__msgtime">{m.time}</span>
                      </div>
                    </div>
                  ))
                )}
              </div>

              {canPost ? (
                <form
                  className="mksocial__composer"
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (!sending && draft.trim()) onSend?.();
                  }}
                >
                  {sendError ? (
                    <p role="alert" className="mksocial__senderr">
                      {sendError}
                    </p>
                  ) : null}
                  <div className="mksocial__composerrow">
                    <textarea
                      className="mksocial__input"
                      rows={1}
                      value={draft}
                      placeholder={composerPlaceholder}
                      onChange={(e) => onDraft?.(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && !e.shiftKey) {
                          e.preventDefault();
                          if (!sending && draft.trim()) onSend?.();
                        }
                      }}
                    />
                    <button
                      type="submit"
                      className="mksocial__send"
                      disabled={sending || !draft.trim()}
                    >
                      {sending ? "Sending\u{2026}" : "Send"}
                    </button>
                  </div>
                </form>
              ) : composerHint ? (
                <div className="mksocial__composerhint" role="status">
                  {composerHint}
                </div>
              ) : null}
            </>
          )}
        </section>
      </div>
    </div>
  );
}

function SocialTabs({ tab, onTab }: { tab: SocialTab; onTab?: (t: SocialTab) => void }) {
  const TABS: { id: SocialTab; label: string }[] = [
    { id: "communities", label: "Communities" },
    { id: "friends", label: "Friends" },
  ];
  return (
    <div className="mksocial__tabs" role="tablist" aria-label="Social sections">
      {TABS.map((t) => (
        <button
          key={t.id}
          type="button"
          role="tab"
          aria-selected={t.id === tab}
          className={"mksocial__tab" + (t.id === tab ? " is-active" : "")}
          onClick={() => onTab?.(t.id)}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}
