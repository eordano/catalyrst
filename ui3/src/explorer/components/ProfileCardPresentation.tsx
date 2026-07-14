import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Avatar, hueFromSeed } from "../../atoms/primitives";
import Button from "../../atoms/Button";
import ConfirmDialog from "./ConfirmDialog";
import type { Relationship } from "../../data/hooks/relationship";
import "./profilecard.css";

export type { Relationship } from "../../data/hooks/relationship";

export type FriendOp = "request" | "accept" | "reject" | "block" | "unblock";

export interface ProfileCardUser {
  address: string;
  name: string;
  picture?: string;
}

const ADDRESS_RE = /^0x[0-9a-fA-F]{6,}$/;

function shortAddr(s: string): string {
  return ADDRESS_RE.test(s) ? `${s.slice(0, 6)}\u{2026}${s.slice(-4)}` : s;
}

function splitName(label: string): { base: string; tag: string } {
  const i = label.indexOf("#");
  return i >= 0 ? { base: label.slice(0, i), tag: label.slice(i) } : { base: label, tag: "" };
}

function isClaimed(name: string): boolean {
  return !!name && !name.includes("#") && !/^0x[0-9a-f]+$/i.test(name);
}

function Verified() {
  return (
    <svg className="pc__verified" viewBox="0 0 16 16" aria-label="verified">
      <defs>
        <linearGradient id="pcv" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ff2d55" />
          <stop offset="1" stopColor="#c640cd" />
        </linearGradient>
      </defs>
      <path d="M8 1l1.7 1.2 2.1-.2 1 1.8 1.9.9-.5 2 .9 1.9-1.6 1.4.1 2.1-2 .6-1.1 1.8-2-.7-2 .7-1.1-1.8-2-.6.1-2.1L1.6 8.6l.9-1.9-.5-2 1.9-.9 1-1.8 2.1.2z" fill="url(#pcv)" />
      <path d="M5.5 8l1.7 1.7L10.8 6" stroke="#fff" strokeWidth="1.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
function CopyIcon() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" aria-hidden="true">
      <rect x="9" y="9" width="11" height="11" rx="2" stroke="currentColor" strokeWidth="1.7" />
      <path d="M5 15V5a2 2 0 0 1 2-2h8" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  );
}
function AddFriendIcon() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" aria-hidden="true">
      <circle cx="9" cy="8" r="3.4" stroke="currentColor" strokeWidth="1.9" />
      <path d="M3.5 19c0-3.1 2.5-4.8 5.5-4.8" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" />
      <path d="M18 8v6M15 11h6" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" />
    </svg>
  );
}
function ViewProfileIcon() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <rect x="3" y="5" width="18" height="14" rx="2.5" stroke="currentColor" strokeWidth="1.8" />
      <circle cx="8.5" cy="12" r="2.2" stroke="currentColor" strokeWidth="1.7" />
      <path d="M13.5 10.5h4M13.5 14h2.5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  );
}
function MentionIcon() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <circle cx="12" cy="12" r="4" stroke="currentColor" strokeWidth="1.8" />
      <path d="M16 8.5V13a2.5 2.5 0 0 0 5 0V12a9 9 0 1 0-3.6 7.2" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}
function BlockIcon() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" aria-hidden="true">
      <circle cx="10" cy="8" r="3.4" stroke="currentColor" strokeWidth="1.8" />
      <path d="M4 19c0-3.2 2.7-5 6-5 1 0 2 .2 2.8.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
      <circle cx="17.5" cy="16.5" r="4.2" stroke="currentColor" strokeWidth="1.8" />
      <path d="M14.7 13.7l5.6 5.6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

type ProfileCardPresentationProps = {
  user: ProfileCardUser;
  x: number;
  y: number;
  me?: { address?: string } | null;
  relationship?: Relationship;
  onFriendAction?: (op: FriendOp, address: string) => void;
  onMention?: (name: string) => void;
  onViewPassport?: (user: ProfileCardUser) => void;
  onClose: () => void;
};

export function ProfileCardPresentation({
  user,
  x,
  y,
  me,
  relationship = "none",
  onFriendAction,
  onMention,
  onViewPassport,
  onClose,
}: ProfileCardPresentationProps) {
  const [copied, setCopied] = useState<"name" | "address" | null>(null);
  const [justSent, setJustSent] = useState(false);
  const [confirmBlock, setConfirmBlock] = useState(false);

  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!justSent) return undefined;
    const t = setTimeout(() => onCloseRef.current(), 1100);
    return () => clearTimeout(t);
  }, [justSent]);

  useEffect(() => {
    if (!copied) return undefined;
    const t = setTimeout(() => setCopied(null), 1200);
    return () => clearTimeout(t);
  }, [copied]);

  const cardRef = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const el = cardRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      left: Math.max(8, Math.min(x, window.innerWidth - r.width - 8)),
      top: Math.max(8, Math.min(y, window.innerHeight - r.height - 8)),
    });
  }, [x, y, relationship]);

  const isMe =
    !!me?.address && !!user.address && me.address.toLowerCase() === user.address.toLowerCase();
  const { base, tag } = splitName(user.name);
  const seed = user.address || user.name;
  const hue = hueFromSeed(seed);
  const color = `hsl(${hue}, 62%, 68%)`;

  const copy = (text: string, which: "name" | "address"): void => {
    navigator.clipboard?.writeText(text).then(
      () => setCopied(which),
      () => {},
    );
  };

  const canUnblock = relationship === "blocked" && !!onFriendAction && !!user.address;
  const canBlock = relationship !== "blocked" && !!onFriendAction && !!user.address;
  const hasDestructive = canUnblock || canBlock;
  const hasMenu = !isMe && (!!onViewPassport || !!onMention || hasDestructive);

  return (
    <>
      <div className="pc__backdrop" onClick={onClose} />
      <div
        ref={cardRef}
        className="pc__card"
        style={{ left: pos.left, top: pos.top }}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Profile"
      >
        <div className="pc__header">
          <Avatar src={user.picture} name={base} seed={seed} size={72} status="online" />
          <button type="button" className="pc__copyrow" title="Copy name" onClick={() => copy(user.name, "name")}>
            <span className="pc__name" style={{ color }}>
              {base}
              {tag && <span className="pc__tag">{tag}</span>}
            </span>
            {isClaimed(user.name) && <Verified />}
            <CopyIcon />
          </button>
          {user.address && (
            <button type="button" className="pc__copyrow pc__addrrow" title="Copy address" onClick={() => copy(user.address, "address")}>
              <span className="pc__addr">{shortAddr(user.address)}</span>
              <CopyIcon />
            </button>
          )}
          {copied && <span className="pc__copied">Copied {copied}</span>}
        </div>

        {!isMe &&
          user.address &&
          onFriendAction &&
          relationship !== "friend" &&
          relationship !== "blocked" &&
          (relationship === "incoming" ? (
            <div className="pc__cta-row">
              <Button className="pc__cta-half" onClick={() => { onFriendAction("accept", user.address); onClose(); }}>
                ACCEPT
              </Button>
              <Button variant="ghost" className="pc__cta-half" onClick={() => { onFriendAction("reject", user.address); onClose(); }}>
                REJECT
              </Button>
            </div>
          ) : justSent || relationship === "requested" ? (
            <div className="pc__cta-sent">&#x2713; REQUEST SENT</div>
          ) : (
            <Button className="pc__cta-full" onClick={() => { onFriendAction("request", user.address); setJustSent(true); }}>
              <AddFriendIcon /> ADD FRIEND
            </Button>
          ))}

        {hasMenu && (
          <div className="pc__menu">
            {onViewPassport && (
              <button type="button" className="pc__row" onClick={() => { onViewPassport(user); onClose(); }}>
                <ViewProfileIcon />
                <span>View Passport</span>
              </button>
            )}
            {onMention && (
              <button type="button" className="pc__row" onClick={() => { onMention(base); onClose(); }}>
                <MentionIcon />
                <span>Mention</span>
              </button>
            )}
            {hasDestructive && <div className="pc__divider" />}
            {canUnblock && (
              <button type="button" className="pc__row" onClick={() => { onFriendAction?.("unblock", user.address); onClose(); }}>
                <BlockIcon />
                <span>Unblock</span>
              </button>
            )}
            {canBlock && (
              <button type="button" className="pc__row pc__danger" onClick={() => setConfirmBlock(true)}>
                <BlockIcon />
                <span>Block</span>
              </button>
            )}
          </div>
        )}
      </div>

      {confirmBlock && (
        <ConfirmDialog
          title={`Block ${base}?`}
          body="Blocked users won't be able to message you, join your community events, or see when you're online."
          confirmLabel="Block"
          cancelLabel="Cancel"
          danger
          onConfirm={() => { setConfirmBlock(false); onFriendAction?.("block", user.address); onClose(); }}
          onCancel={() => setConfirmBlock(false)}
        />
      )}
    </>
  );
}
