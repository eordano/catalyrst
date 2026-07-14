// React DOM chat -- Explorer 2.0 design, three states:
//   - collapsed (hidden): a borderless translucent input bar; focusing it opens chat
//   - open + idle (not hovered/focused): translucent -- bubbles float over the world
//   - open + active (hover/focus): full solid panel -- navbar, emoji, members, borders
// Incoming messages come from useBridgeState().chat; sends go via sendBridge("SendChat", ...).

import type { ButtonHTMLAttributes, KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent, ReactNode } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { NearbyPlayer } from "../../generated/bridge/NearbyPlayer";
import { Avatar } from "../../atoms/primitives";
import DclLogomark from "../../atoms/DclLogomark";
import type { BridgeChatLine } from "../../overlay/bridge";
import { EmojiPicker } from "./EmojiPicker";
import { getEmojiData, loadEmojiData, searchByShortcode, SHORTCODE_RE, type Emoji } from "./emojiData";
import { MessageText, mentionsMe, buildNameIndex } from "./chatText";
import type { ProfileCardProps } from "../components/ProfileCard";
import type { ProfileCardUser } from "../components/ProfileCardPresentation";
import styles from "./Chat.module.css";

// The chat's world: everything it reads and every action it fires, injected so
// the same view can sit over the explorer bridge (default export) or any other
// room transport (a host's own page rooms). Optional actions render inert.
export type ChatIo = {
  chat: BridgeChatLine[];
  players: NearbyPlayer[];
  me: { address: string; name: string } | null;
  blocked: string[];
  live: boolean;
  send: (message: string) => void;
  teleport?: (x: number, z: number) => void;
  changeRealm?: (realm: string) => void;
};

const MAX_LEN = 500;
const PARCEL_SIZE = 16;
const ADDRESS_RE = /^0x[0-9a-fA-F]{6,}$/;

// DCL rarity name colors -- gives each sender a stable, on-brand color.
const RARITY = [
  "#73d3d3", "#acf8f8", "#ff8362", "#ff4bed", "#caff73", "#a14bf3",
  "#e8b9ff", "#fea217", "#81e1ff", "#ff7439", "#ffa25a", "#ffc95b",
  "#a0abff", "#c640cd",
];
const SYSTEM_COLOR = "#61d04f";

function isSystem(sender: string): boolean {
  return !sender || sender.toLowerCase() === "system";
}

function shortAddr(s: string): string {
  return ADDRESS_RE.test(s) ? `${s.slice(0, 6)}\u{2026}${s.slice(-4)}` : s;
}

function displaySender(sender: string): string {
  if (isSystem(sender)) return "DCL System";
  return shortAddr(sender);
}

function memberLabel(m: NearbyPlayer): string {
  return m.name.trim() ? m.name : shortAddr(m.address);
}

function findMember(members: NearbyPlayer[], address: string): NearbyPlayer | undefined {
  return members.find((m) => m.address.toLowerCase() === address.toLowerCase());
}

/** Split "Name#a1b2" into the colored base and a dimmer #tag. */
function splitName(label: string): { base: string; tag: string } {
  const i = label.indexOf("#");
  return i >= 0 ? { base: label.slice(0, i), tag: label.slice(i) } : { base: label, tag: "" };
}

function hash(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  return h;
}

function senderColor(sender: string): string {
  if (isSystem(sender)) return SYSTEM_COLOR;
  return RARITY[hash(sender) % RARITY.length] ?? SYSTEM_COLOR;
}

/** Our Avatar atom takes a numeric hue rather than upstream's raw CSS color -- map the
 *  rarity hex into a hue so the avatar tint and the colored sender name stay in sync. */
function hexToHue(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  const r = ((n >> 16) & 255) / 255;
  const g = ((n >> 8) & 255) / 255;
  const b = (n & 255) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  if (max === min) return 0;
  const d = max - min;
  let h: number;
  if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return Math.round(h * 60);
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function dayKey(ts: number): string {
  const d = new Date(ts);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

function formatDay(ts: number): string {
  if (dayKey(ts) === dayKey(Date.now())) return "Today";
  return new Date(ts).toLocaleDateString([], {
    weekday: "short",
    day: "numeric",
    month: "short",
  });
}

function PersonIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="8" cy="5" r="2.6" stroke="currentColor" strokeWidth="1.4" />
      <path
        d="M3.2 13c0-2.4 2.1-3.8 4.8-3.8s4.8 1.4 4.8 3.8"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
      />
    </svg>
  );
}

function Smiley() {
  return (
    <svg width="20" height="20" viewBox="0 0 22 22" fill="none" aria-hidden="true">
      <circle cx="11" cy="11" r="10" stroke="currentColor" strokeWidth="2" />
      <circle cx="7.6" cy="9" r="1.15" fill="currentColor" />
      <circle cx="14.4" cy="9" r="1.15" fill="currentColor" />
      <path
        d="M7 13.4c1 1.6 2.5 2.4 4 2.4s3-.8 4-2.4"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
      />
    </svg>
  );
}

function CharRing({ len }: { len: number }) {
  const pct = Math.min(1, len / MAX_LEN);
  const r = 8;
  const circ = 2 * Math.PI * r;
  const color = pct >= 0.9 ? "var(--brand)" : pct >= 0.7 ? "var(--gold)" : "var(--green)";
  return (
    <svg className={styles.ring} width="20" height="20" viewBox="0 0 20 20" aria-hidden="true">
      <circle cx="10" cy="10" r={r} fill="none" stroke="rgba(255,255,255,0.25)" strokeWidth="2" />
      <circle
        cx="10"
        cy="10"
        r={r}
        fill="none"
        stroke={color}
        strokeWidth="2"
        strokeLinecap="round"
        strokeDasharray={circ}
        strokeDashoffset={circ * (1 - pct)}
        transform="rotate(-90 10 10)"
      />
    </svg>
  );
}

/** Local stand-in for upstream's shared design/ControlButton -- same look, module-local. */
function CtrlButton({
  variant = "ghost",
  shape = "square",
  size = "md",
  active = false,
  className = "",
  ...rest
}: {
  variant?: "ghost" | "solid";
  shape?: "square" | "circle" | "pill";
  size?: "sm" | "md";
  active?: boolean;
} & ButtonHTMLAttributes<HTMLButtonElement>) {
  const variantClass = variant === "solid" ? styles.ctrlSolid : styles.ctrlGhost;
  const shapeClass = shape === "circle" ? styles.ctrlCircle : shape === "pill" ? styles.ctrlPill : styles.ctrlSquare;
  const sizeClass = size === "sm" ? styles.ctrlSm : styles.ctrlMd;
  return (
    <button
      type="button"
      className={`${styles.ctrlBtn} ${variantClass} ${shapeClass} ${sizeClass} ${active ? styles.ctrlActive : ""} ${className}`.trim()}
      aria-pressed={active}
      {...rest}
    />
  );
}

export function DaySeparator({ ts }: { ts: number }) {
  return (
    <div className={styles.dayRow}>
      <span className={styles.dayPill}>{formatDay(ts)}</span>
    </div>
  );
}

const MSG_STYLES = { url: styles.url, mention: styles.mention, location: styles.location, world: styles.world };

type ChatLine = { id: string; ts: number; sender: string; senderName?: string; message: string };

export function ChatBubble({
  line,
  name,
  members = [],
  me,
  onOpenProfile,
  onLocation,
  onVisitWorld,
}: {
  line: ChatLine;
  name: string;
  members?: NearbyPlayer[];
  me?: { address?: string; name?: string } | null;
  /** Open the profile viewer for a user, anchored at the click. */
  onOpenProfile?: (user: ProfileCardUser, e: ReactMouseEvent) => void;
  /** A location link (x,y) in the message was clicked -> teleport. */
  onLocation?: (x: number, y: number) => void;
  /** A world name (e.g. boedo.dcl.eth) in the message was clicked -> prompt to jump there. */
  onVisitWorld?: (name: string) => void;
}) {
  const color = senderColor(line.sender);
  const { base, tag } = splitName(name);
  const senderMember = findMember(members, line.sender);
  const sender: ProfileCardUser = { address: line.sender, name, picture: senderMember?.picture };
  const highlight = mentionsMe(line.message, me ?? null, buildNameIndex(members));

  // Open the profile menu -- on left-click OR right-click (suppress the browser menu).
  const openSender = (e: ReactMouseEvent): void => {
    if (e.type === "contextmenu") e.preventDefault();
    onOpenProfile?.(sender, e);
  };
  // Clicking an @mention opens that user's profile (resolved against the roster).
  const onMention = (address: string, mname: string, e: ReactMouseEvent): void => {
    if (e.type === "contextmenu") e.preventDefault();
    const m = findMember(members, address);
    onOpenProfile?.({ address, name: m?.name || `@${mname}`, picture: m?.picture }, e);
  };

  return (
    <div className={`${styles.entry} ${highlight ? styles.mentionMe : ""}`.trim()}>
      <button type="button" className={styles.avatarBtn} aria-label={`View ${base}`} onClick={openSender} onContextMenu={openSender}>
        <Avatar src={senderMember?.picture} name={name} hue={hexToHue(color)} size={28} />
      </button>
      <div className={styles.bubble}>
        <button type="button" className={styles.name} style={{ color }} onClick={openSender} onContextMenu={openSender}>
          {base}
          {tag && <span className={styles.tag}>{tag}</span>}
        </button>
        <span className={styles.text}>
          <MessageText text={line.message} members={members} styles={MSG_STYLES} onMention={onMention} onLocation={(x, y) => onLocation?.(x, y)} onWorld={onVisitWorld} />
        </span>
        <span className={styles.time}>{formatTime(line.ts)}</span>
      </div>
    </div>
  );
}

export function MemberRow({ member }: { member: NearbyPlayer }) {
  const { base, tag } = splitName(memberLabel(member));
  const color = senderColor(member.address);
  return (
    <div className={styles.memberRow}>
      <Avatar src={member.picture} name={base} hue={hexToHue(color)} size={40} status="online" />
      <div className={styles.memberInfo}>
        <span className={styles.memberName} style={{ color }}>
          {base}
          {tag && <span className={styles.tag}>{tag}</span>}
        </span>
        <span className={styles.memberStatus}>Online</span>
      </div>
    </div>
  );
}

function MembersOverlay({
  members,
  onBack,
  onClose,
  membersTitle = "Nearby",
  membersEmpty = "No one nearby",
}: {
  members: NearbyPlayer[];
  onBack: () => void;
  onClose: () => void;
  membersTitle?: string;
  membersEmpty?: string;
}) {
  return (
    <div className={styles.membersPanel}>
      <header className={styles.membersHeader}>
        <CtrlButton variant="ghost" className={styles.glyphLg} aria-label="Back" onClick={onBack}>
          &#x2039;
        </CtrlButton>
        <span className={styles.membersTitle}>{membersTitle}</span>
        <span className={styles.membersCount}>
          <span className={styles.personIcon} aria-hidden="true">
            &#x25CF;
          </span>
          {members.length} Online
        </span>
        <CtrlButton variant="solid" className={styles.glyph} aria-label="Close chat" onClick={onClose}>
          &#xD7;
        </CtrlButton>
      </header>
      <div className={styles.membersList}>
        {members.length === 0 ? (
          <div className={styles.empty}>{membersEmpty}</div>
        ) : (
          members.map((m) => <MemberRow key={m.address} member={m} />)
        )}
      </div>
    </div>
  );
}

export function ChatView({
  open,
  onToggle,
  hidden = false,
  io,
  docked = false,
  title = "Nearby",
  membersTitle,
  membersEmpty,
  emptyLine,
  profileCard,
}: {
  open: boolean;
  onToggle: () => void;
  /** Friends (and other left-docked panels) share chat's bottom-left dock; hide it
   *  entirely when one is open so they don't overlap. */
  hidden?: boolean;
  io: ChatIo;
  /** Static in-flow layout for hosts that dock the chat in their own chrome. */
  docked?: boolean;
  title?: string;
  /** Members-panel header and empty-state copy -- a page-room host replaces the
   *  explorer's "Nearby" vocabulary; explorer defaults stay put. */
  membersTitle?: string;
  membersEmpty?: string;
  /** The empty chat line; hosts whose rooms keep no history state that here. */
  emptyLine?: string;
  /** The member/sender profile popup; a host with no profiles omits it and the
   *  click simply does nothing extra. The explorer wrapper passes the real one. */
  profileCard?: (props: ProfileCardProps) => ReactNode;
}) {
  const [draft, setDraft] = useState("");
  const [picker, setPicker] = useState(false);
  const [showMembers, setShowMembers] = useState(false);
  const [emojiReady, setEmojiReady] = useState(() => getEmojiData() != null);
  const [scQuery, setScQuery] = useState<string | null>(null);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [mentionQuery, setMentionQuery] = useState<string | null>(null);
  const [mentionSug, setMentionSug] = useState<NearbyPlayer[]>([]);
  const [profileTarget, setProfileTarget] = useState<{ user: ProfileCardUser; x: number; y: number } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const { chat: chatPushes, players, me, blocked, live } = io;
  const ProfileCardImpl = profileCard;
  const members = players;

  // Emoji dataset is lazy-loaded -- fetch it the first time the picker opens or a
  // ":shortcode" is being typed, then suggestions recompute once it lands.
  useEffect(() => {
    if (emojiReady || (scQuery == null && !picker)) return undefined;
    let alive = true;
    void loadEmojiData().then(() => {
      if (alive) setEmojiReady(true);
    });
    return () => {
      alive = false;
    };
  }, [emojiReady, scQuery, picker]);
  const suggestions = useMemo<Emoji[]>(
    () => (emojiReady && scQuery ? searchByShortcode(scQuery) : []),
    [emojiReady, scQuery],
  );

  // "active" = the user is interacting -> show the full solid panel + chrome.
  const active = open && (hovered || focused || picker);
  const bare = !active; // collapsed or idle-open -> borderless translucent input only

  const nameByAddr = useMemo(() => {
    const m = new Map<string, string>();
    for (const mem of members) if (mem.name.trim()) m.set(mem.address.toLowerCase(), mem.name);
    return m;
  }, [members]);
  // Roster name wins (freshest); else fall back to the name the push itself carried
  // (covers the gap before a just-joined sender shows up in the nearby roster); else
  // format the address/id.
  const resolveName = (sender: string, pushedName?: string): string =>
    nameByAddr.get(sender.toLowerCase()) ?? (pushedName?.trim() || displaySender(sender));

  // the blocked set is the union of both directions, and filtering at render time means
  // a blocked sender's messages leave the list retroactively, not just from the block onward
  const blockedSet = useMemo(
    () => new Set(blocked.map((a) => a.toLowerCase())),
    [blocked],
  );
  const lines = useMemo<ChatLine[]>(
    () =>
      chatPushes
        .filter((l) => !(l.senderAddress && blockedSet.has(l.senderAddress.toLowerCase())))
        .map((l, i) => {
          const sender = l.senderAddress || l.senderName || "system";
          return {
            id: l.timestamp != null ? `t${l.timestamp}-${i}` : `i${sender}|${l.message ?? ""}|${i}`,
            ts: l.timestamp ?? Date.now(),
            sender,
            senderName: l.senderName ?? undefined,
            message: l.message ?? "",
          };
        }),
    [chatPushes, blockedSet],
  );

  const rows = useMemo(() => {
    const out: ({ kind: "day"; ts: number; id: string } | { kind: "msg"; line: ChatLine })[] = [];
    let prev = "";
    for (const line of lines) {
      const key = dayKey(line.ts);
      if (key !== prev) {
        out.push({ kind: "day", ts: line.ts, id: `day-${key}` });
        prev = key;
      }
      out.push({ kind: "msg", line });
    }
    return out;
  }, [lines]);

  useEffect(() => {
    if (!open) return;
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines, open, active]);

  // Opening chat (e.g. via the sidebar icon) focuses the input so it comes up in the
  // active/focused state, ready to type -- matches Unity's "click chat -> start typing".
  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open]);

  // Leaving the chat (click outside -> not hovered/focused) resets the nearby-members
  // overlay, so re-entering shows messages -- not the members list left open from before.
  useEffect(() => {
    if (!active) setShowMembers(false);
  }, [active]);

  const openIfClosed = (): void => {
    if (!open) onToggle();
  };

  // Profile viewer: clicking a name/avatar/@mention opens the shared profile card at the click.
  const openProfile = (user: ProfileCardUser, e: ReactMouseEvent): void => {
    setProfileTarget({ user, x: e.clientX, y: e.clientY });
  };
  // "Mention" from the viewer drops @name into the draft, ready to send.
  const insertMention = (name: string): void => {
    setDraft((d) => `${d.replace(/\s*$/, "")} @${name} `.trimStart());
    openIfClosed();
    inputRef.current?.focus();
  };

  const MENTION_RE = /@([\w-]*)$/;
  const updateDraft = (value: string): void => {
    setDraft(value);
    const m = value.match(SHORTCODE_RE);
    const code = m?.[1];
    setScQuery(code ?? null);
    // @mention autocomplete from the nearby roster (trailing @partial).
    const mm = value.match(MENTION_RE);
    const mention = mm?.[1];
    setMentionQuery(mention ?? null);
    const q = (mention ?? "").toLowerCase();
    setMentionSug(mm ? members.filter((p) => (p.name || p.address).toLowerCase().includes(q)).slice(0, 6) : []);
  };

  const applyMention = (member: NearbyPlayer): void => {
    const label = member.name.trim() ? member.name.split("#")[0] : member.address;
    setDraft((d) => d.replace(MENTION_RE, `@${label} `));
    setMentionSug([]);
    setMentionQuery(null);
    inputRef.current?.focus();
  };

  const applyEmoji = (glyph: string): void => {
    setDraft((d) => {
      const m = d.match(SHORTCODE_RE);
      return (m ? d.slice(0, m.index) : d) + glyph;
    });
    setScQuery(null);
    inputRef.current?.focus();
  };

  const send = (): void => {
    const message = draft.trim();
    if (!message) return;
    io.send(message);
    setDraft("");
    setScQuery(null);
    setMentionSug([]);
    setMentionQuery(null);
  };

  const onKeyDown = (e: ReactKeyboardEvent): void => {
    e.stopPropagation(); // keep movement keys out of the engine while typing
    if (e.key === "Enter") {
      e.preventDefault();
      const firstMention = mentionSug[0];
      const firstEmoji = suggestions[0];
      if (firstMention) applyMention(firstMention);
      else if (firstEmoji) applyEmoji(firstEmoji.emoji);
      else send();
    } else if (e.key === "Escape") {
      if (mentionQuery != null) {
        setMentionQuery(null);
        setMentionSug([]);
      } else if (scQuery != null) {
        setScQuery(null);
      } else if (picker) setPicker(false);
    }
  };

  const toggleEmoji = (): void => {
    openIfClosed();
    setPicker((p) => !p);
  };

  const onLocation = (x: number, y: number): void => {
    io.teleport?.(x * PARCEL_SIZE + PARCEL_SIZE / 2, y * PARCEL_SIZE + PARCEL_SIZE / 2);
  };
  const onVisitWorld = (name: string): void => {
    io.changeRealm?.(name);
  };

  if (hidden) return null;

  return (
    <div
      className={`${styles.root} ${docked ? styles.docked : ""} ${open ? styles.open : ""} ${active ? styles.active : ""}`.trim()}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {open && active && (
        <header className={styles.nav}>
          <div className={styles.navLeft}>
            <DclLogomark size={26} className={styles.channelIcon} />
            <span className={styles.navTitle}>{title}</span>
          </div>
          <div className={styles.navRight}>
            {members.length > 0 && (
              <CtrlButton
                variant="ghost"
                shape="pill"
                active={showMembers}
                aria-label={`${members.length} nearby`}
                onClick={() => setShowMembers((s) => !s)}
              >
                <PersonIcon />
                {members.length}
              </CtrlButton>
            )}
            <CtrlButton variant="solid" className={styles.glyph} aria-label="Close chat" onClick={onToggle}>
              &#xD7;
            </CtrlButton>
          </div>
        </header>
      )}

      {open && (
        <div ref={listRef} className={styles.messages}>
          {rows.length === 0 ? (
            <div className={styles.empty}>
              {live
                ? emptyLine ?? `No messages yet \u{2014} say hello to ${title}.`
                : `Connecting to ${title} chat\u{2026}`}
            </div>
          ) : (
            rows.map((r) =>
              r.kind === "day" ? (
                <DaySeparator key={r.id} ts={r.ts} />
              ) : (
                <ChatBubble
                  key={r.line.id}
                  line={r.line}
                  name={resolveName(r.line.sender, r.line.senderName)}
                  members={members}
                  me={me}
                  onOpenProfile={openProfile}
                  onLocation={onLocation}
                  onVisitWorld={onVisitWorld}
                />
              ),
            )
          )}
        </div>
      )}

      {active && picker && (
        <div className={styles.pickerWrap}>
          <EmojiPicker onPick={applyEmoji} onClose={() => setPicker(false)} />
        </div>
      )}

      {active && mentionQuery != null && mentionSug.length > 0 && (
        <div className={styles.suggest}>
          <ul className={styles.suggestList}>
            {mentionSug.map((m, i) => (
              <li key={m.address}>
                <button
                  type="button"
                  className={`${styles.suggestItem} ${i === 0 ? styles.suggestActive : ""}`.trim()}
                  onClick={() => applyMention(m)}
                >
                  <Avatar src={m.picture} name={m.name || m.address} hue={hexToHue(senderColor(m.address))} size={20} />
                  <span className={styles.suggestName}>{m.name || shortAddr(m.address)}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {active && scQuery != null && emojiReady && (
        <div className={styles.suggest}>
          {suggestions.length === 0 ? (
            <div className={styles.noResults}>No results</div>
          ) : (
            <ul className={styles.suggestList}>
              {suggestions.map((e, i) => (
                <li key={e.code}>
                  <button
                    type="button"
                    className={`${styles.suggestItem} ${i === 0 ? styles.suggestActive : ""}`.trim()}
                    onClick={() => applyEmoji(e.emoji)}
                  >
                    <span className={styles.suggestGlyph}>{e.emoji}</span>
                    <span className={styles.suggestName}>{e.expression}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      <form
        className={styles.inputRow}
        onSubmit={(e) => {
          e.preventDefault();
          send();
        }}
      >
        <input
          ref={inputRef}
          className={`${styles.input} ${bare ? styles.inputBare : ""}`.trim()}
          value={draft}
          onChange={(e) => updateDraft(e.target.value)}
          onFocus={() => {
            setFocused(true);
            openIfClosed();
          }}
          onBlur={() => setFocused(false)}
          placeholder={focused ? "Message Nearby" : "Press Enter to chat"}
          maxLength={MAX_LEN}
          onKeyDown={onKeyDown}
          aria-label="Send a message to Nearby chat"
        />
        {!bare && draft.length > 0 && <CharRing len={draft.length} />}
        {!bare && (
          <CtrlButton variant="ghost" size="sm" active={picker} className={styles.emojiBtn} aria-label="Emoji" onClick={toggleEmoji}>
            <Smiley />
          </CtrlButton>
        )}
      </form>

      {open && active && showMembers && (
        <MembersOverlay
          members={members}
          membersTitle={membersTitle}
          membersEmpty={membersEmpty}
          onBack={() => setShowMembers(false)}
          onClose={() => {
            setShowMembers(false);
            onToggle();
          }}
        />
      )}

      {profileTarget && ProfileCardImpl && (
        <ProfileCardImpl
          user={profileTarget.user}
          x={profileTarget.x}
          y={profileTarget.y}
          onMention={(name) => {
            insertMention(name);
            setProfileTarget(null);
          }}
          onClose={() => setProfileTarget(null)}
        />
      )}
    </div>
  );
}

