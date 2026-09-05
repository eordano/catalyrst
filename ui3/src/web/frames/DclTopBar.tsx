import type { CSSProperties } from "react";
import { useEffect, useRef, useState } from "react";
import { asset } from "../../asset";
import { ChevronDown } from "../../atoms/icons";
import { catalystBase, getJSON } from "../../data/catalyst/client";
import {
  notificationBody,
  notificationLink,
  notificationTitle,
  parseNotificationsLoose,
  relativeTime,
  type Notification,
} from "../../data/catalyst/notificationsView";
import { useChromeAuth } from "./chrome-auth";
import "./dcltopbar.css";

type DclTopBarVariant = "default" | "dao" | "sites";

type NavMenuItem = { label: string; href: string };
type NavLink = { id: string; label: string; href: string; caret?: boolean; menu?: NavMenuItem[] };

const LEARN_MENU: NavMenuItem[] = [
  // Absolute: the docs site is not proxied on every sub-origin (some 404 on
  // /docs/), and docs.decentraland.org is the canonical home anyway.
  { label: "Docs", href: "https://docs.decentraland.org/" },
  { label: "Blog", href: "/blog" },
];

const LINK_DEFS = {
  explore: { id: "explore", label: "Explore", href: "/discover" },
  whatson: { id: "whatson", label: "What's On", href: "/discover" },
  shop: { id: "shop", label: "Shop", href: "/shop" },
  create: { id: "create", label: "Create", href: "/create" },
  learn: { id: "learn", label: "Learn", href: "/blog", caret: true, menu: LEARN_MENU },
  vote: { id: "vote", label: "Vote", href: "/governance" },
  events: { id: "events", label: "Events", href: "/discover" },
} satisfies Record<string, NavLink>;

export type DclTopBarNavId = keyof typeof LINK_DEFS;

const BellMark = ({ size = 19 }: { size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor"
    strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M18 8a6 6 0 1 0-12 0c0 7-3 9-3 9h18s-3-2-3-9" />
    <path d="M13.7 21a2 2 0 0 1-3.4 0" />
  </svg>
);

const BurgerMark = ({ open, size = 22 }: { open: boolean; size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor"
    strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {open ? <path d="M6 6l12 12M18 6 6 18" /> : <path d="M3 6h18M3 12h18M3 18h18" />}
  </svg>
);

export const DCL_LINKS_BY_VARIANT: Record<DclTopBarVariant, NavLink[]> = {
  default: (["explore", "shop", "create", "learn"] as const).map((id) => LINK_DEFS[id]),
  dao: (["shop", "create", "learn", "vote", "events"] as const).map((id) => LINK_DEFS[id]),
  sites: (["whatson", "shop", "create", "learn"] as const).map((id) => LINK_DEFS[id]),
};

type NotifState =
  | { phase: "idle" | "loading" }
  | { phase: "error" }
  | { phase: "ready"; rows: Notification[] };

type DclTopBarProps = {
  variant?: DclTopBarVariant;
  active?: DclTopBarNavId | "";
  signedIn?: boolean;
  account?: string;
  transparent?: boolean;
  onSignIn?: () => void;
  signInHref?: string;
  fetchNotifications?: () => Promise<unknown>;
};

export default function DclTopBar({
  variant = "default",
  active = "shop",
  signedIn = undefined,
  account = undefined,
  transparent = false,
  onSignIn = undefined,
  signInHref = "/marketplace/account",
  fetchNotifications = undefined,
}: DclTopBarProps) {
  const auth = useChromeAuth();
  const isIn = signedIn ?? auth.signedIn;
  const acct = account ?? auth.account;
  const doSignIn = onSignIn ?? auth.onSignIn;
  const fetchNotifs = fetchNotifications ?? auth.fetchNotifications;
  const links = DCL_LINKS_BY_VARIANT[variant] ?? DCL_LINKS_BY_VARIANT.default;
  const [menuOpen, setMenuOpen] = useState(false);
  const [openDropdown, setOpenDropdown] = useState("");
  const ddEscRef = useRef(false);

  const [faceUrl, setFaceUrl] = useState("");
  useEffect(() => {
    setFaceUrl("");
    if (!isIn || !acct) return;
    let cancelled = false;
    void (async () => {
      try {
        const addr = acct.trim().toLowerCase();
        const raw = await getJSON(`/lambdas/profile/${encodeURIComponent(addr)}`);
        const face = (raw as { avatars?: Array<{ avatar?: { snapshots?: { face256?: unknown } } }> })
          ?.avatars?.[0]?.avatar?.snapshots?.face256;
        if (cancelled || typeof face !== "string" || !face) return;
        const url =
          /^https?:\/\//i.test(face) || face.startsWith("data:")
            ? face
            : `${catalystBase()}/content/contents/${face}`;
        setFaceUrl(url);
      } catch {
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isIn, acct]);

  const [bellOpen, setBellOpen] = useState(false);
  const [notif, setNotif] = useState<NotifState>({ phase: "idle" });
  const bellRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!bellOpen) return;
    const onDown = (e: MouseEvent) => {
      if (bellRef.current && !bellRef.current.contains(e.target as Node)) setBellOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setBellOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [bellOpen]);

  const openBell = () => {
    if (!isIn) {
      doSignIn?.();
      return;
    }
    const next = !bellOpen;
    setBellOpen(next);
    if (next && fetchNotifs) {
      setNotif({ phase: "loading" });
      fetchNotifs()
        .then((raw) => setNotif({ phase: "ready", rows: parseNotificationsLoose(raw) }))
        .catch(() => setNotif({ phase: "error" }));
    } else if (next) {
      setNotif({ phase: "error" });
    }
  };

  const now = Date.now();

  const renderNavLink = (link: NavLink) => {
    if (!link.menu?.length) {
      return (
        <a
          key={link.id}
          href={link.href}
          className={"dtb__link" + (link.id === active ? " is-active" : "")}
          aria-current={link.id === active ? "page" : undefined}
        >
          {link.label}
        </a>
      );
    }
    const open = openDropdown === link.id;
    return (
      <div
        key={link.id}
        className="dtb__dd"
        onMouseEnter={() => setOpenDropdown(link.id)}
        onMouseLeave={() => setOpenDropdown((cur) => (cur === link.id ? "" : cur))}
        onFocus={() => {
          if (!ddEscRef.current) setOpenDropdown(link.id);
        }}
        onBlur={(e) => {
          if (!e.currentTarget.contains(e.relatedTarget as Node)) {
            setOpenDropdown((cur) => (cur === link.id ? "" : cur));
          }
        }}
        onKeyDown={(e) => {
          if (e.key !== "Escape") return;
          setOpenDropdown((cur) => (cur === link.id ? "" : cur));
          ddEscRef.current = true;
          e.currentTarget.querySelector<HTMLAnchorElement>("a.dtb__link")?.focus();
          ddEscRef.current = false;
        }}
      >
        <a
          href={link.href}
          className={"dtb__link" + (link.id === active ? " is-active" : "")}
          aria-current={link.id === active ? "page" : undefined}
          aria-haspopup="menu"
          aria-expanded={open}
        >
          {link.label}
          <ChevronDown size={13} className="dtb__linkcaret" />
        </a>
        <div className={"dtb__ddmenu" + (open ? " is-open" : "")} role="menu" aria-label={`${link.label} menu`}>
          {link.menu.map((item) => (
            <a key={item.label} href={item.href} className="dtb__dditem" role="menuitem">
              {item.label}
            </a>
          ))}
        </div>
      </div>
    );
  };

  return (
    <header className={"dtb" + (transparent ? " dtb--transparent" : "")} role="banner" aria-label="Decentraland">
      <a className="dtb__brand" href="/" aria-label="Decentraland">
        <img src={asset("assets/dcl-logo.png")} alt="" />
      </a>

      <button
        type="button"
        className="dtb__burger"
        aria-label="Toggle navigation menu"
        aria-expanded={menuOpen}
        onClick={() => setMenuOpen((o) => !o)}
      >
        <BurgerMark open={menuOpen} />
      </button>

      <nav className="dtb__links" aria-label="Decentraland sections">
        {links.map(renderNavLink)}
      </nav>

      <div className="dtb__right">
        {variant === "dao" && (
          <a className="dtb__download" href="/play/" style={{ textDecoration: "none" }}>JUMP IN</a>
        )}
        {isIn || doSignIn ? (
          <div className="dtb__bellwrap" ref={bellRef}>
            <button
              type="button"
              className="dtb__bell"
              aria-label="Notifications"
              aria-haspopup={isIn ? "dialog" : undefined}
              aria-expanded={isIn ? bellOpen : undefined}
              onClick={openBell}
            >
              <BellMark />
            </button>
            {bellOpen && (
              <div className="dtb__notif" role="dialog" aria-label="Notifications">
                <div className="dtb__notifhead">Notifications</div>
                {notif.phase === "loading" && <div className="dtb__notifmsg">Loading&#x2026;</div>}
                {notif.phase === "error" && (
                  <div className="dtb__notifmsg">Couldn&rsquo;t load notifications.</div>
                )}
                {notif.phase === "ready" && notif.rows.length === 0 && (
                  <div className="dtb__notifmsg">No notifications yet</div>
                )}
                {notif.phase === "ready" && notif.rows.length > 0 && (
                  <ul className="dtb__notiflist">
                    {notif.rows.slice(0, 10).map((n) => {
                      const link = notificationLink(n);
                      const body = (
                        <>
                          <span className="dtb__notifdot" aria-hidden="true" />
                          <span className="dtb__notifbody">
                            <span className="dtb__notiftitle">{notificationTitle(n)}</span>
                            {notificationBody(n) && (
                              <span className="dtb__notifdesc">{notificationBody(n)}</span>
                            )}
                          </span>
                          <span className="dtb__notiftime">{relativeTime(n.timestamp, now)}</span>
                        </>
                      );
                      return (
                        <li key={n.id} className={"dtb__notifitem" + (n.read ? "" : " is-unread")}>
                          {link ? (
                            <a className="dtb__notiflink" href={link}>
                              {body}
                            </a>
                          ) : (
                            body
                          )}
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>
            )}
          </div>
        ) : null}
        {isIn ? (
          <a className="dtb__usermenu" href={signInHref} aria-label="My account" style={{ textDecoration: "none" }}>
            <span className="dtb__avatar u-avatar" style={{ "--sz": "40px", "--hue": 268 } as CSSProperties}>
              {faceUrl && (
                <img
                  className="u-avatar__img"
                  src={faceUrl}
                  alt=""
                  width="40"
                  height="40"
                  loading="lazy"
                  onError={() => setFaceUrl("")}
                />
              )}
            </span>
          </a>
        ) : doSignIn ? (
          <button type="button" className="dtb__signin" onClick={() => doSignIn()}>SIGN IN</button>
        ) : (
          <a className="dtb__signin" href={signInHref} style={{ textDecoration: "none" }}>SIGN IN</a>
        )}
      </div>

      <nav className={"dtb__menu" + (menuOpen ? " is-open" : "")} aria-label="Decentraland sections">
        {links.map((link) => (
          <span key={link.id} className="dtb__menugroup">
            <a
              href={link.href}
              className={"dtb__menulink" + (link.id === active ? " is-active" : "")}
              aria-current={link.id === active ? "page" : undefined}
              onClick={() => setMenuOpen(false)}
            >
              {link.label}
              {link.caret && <ChevronDown size={16} className="dtb__menucaret" />}
            </a>
            {link.menu?.map((item) => (
              <a
                key={item.label}
                href={item.href}
                className="dtb__menulink dtb__menulink--sub"
                onClick={() => setMenuOpen(false)}
              >
                {item.label}
              </a>
            ))}
          </span>
        ))}
      </nav>
    </header>
  );
}
