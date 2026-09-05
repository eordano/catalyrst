import type { FormEvent, ReactNode } from "react";
import { useEffect, useId, useRef, useState } from "react";
import DclTopBar, { type DclTopBarNavId } from "./DclTopBar";
import { useChromeAuth } from "./chrome-auth";
import { Caret } from "../../atoms/icons";
import { siteHost } from "../../data/site";
import "./siteschrome.css";

export type SitesNavId =
  | "play"
  | "marketplace"
  | "docs"
  | "dao"
  | "legal"
  | "create"
  | "explore"
  | "whatson"
  | "shop"
  | "learn";

const ACTIVE_MAP: Record<SitesNavId, DclTopBarNavId | ""> = {
  play: "whatson",
  marketplace: "shop",
  docs: "learn",
  dao: "",
  legal: "",
  create: "create",
  explore: "whatson",
  whatson: "whatson",
  shop: "shop",
  learn: "learn",
};

type FooterLink = { label: string; href: string };
type FooterCol = { title: string; links: FooterLink[] };

const FOOTER_COLS: FooterCol[] = [
  {
    title: "Getting Started",
    links: [
      { label: "What is Decentraland?", href: "/for" },
      { label: "Jump into Decentraland", href: "/play/" },
      { label: "System Requirements", href: "/play/" },
      { label: "FAQs", href: "/support" },
      { label: "Contact Support", href: "/support" },
    ],
  },
  {
    title: "Resources",
    links: [
      { label: "Shop", href: "/shop" },
      { label: "Creator Hub", href: "/creator-hub" },
      // Absolute: /docs/ is not proxied on every sub-origin; docs.decentraland.org
      // is the canonical docs home.
      { label: "Docs", href: "https://docs.decentraland.org/" },
      { label: "Blog", href: "/blog" },
      { label: "Vote", href: "/governance" },
    ],
  },
];

const LEGAL: FooterLink[] = [
  { label: "Privacy Policy", href: "/privacy" },
  { label: "Terms of Use", href: "/terms" },
  { label: "Content Policy", href: "/content" },
  { label: "Code of Ethics", href: "/ethics" },
];

type Language = { code: string; label: string; flag: string };

const DEFAULT_LANG: Language = { code: "en", label: "English", flag: "1f1ec-1f1e7" };
const LANGUAGES: Language[] = [
  DEFAULT_LANG,
  { code: "es", label: "Espa\u{F1}ol", flag: "1f1ea-1f1f8" },
  { code: "zh", label: "\u{4E2D}\u{6587}", flag: "1f1e8-1f1f3" },
];

const LANG_STORAGE_KEY = "dcl:lang";

function flagUrl(flag: string): string {
  return `/assets/twemoji-${flag}.svg`;
}

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

type Social = { name: string; href: string; path: string };

const SOCIALS: Social[] = [
  {
    name: "Discord",
    href: "https://dcl.gg/discord",
    path: "M19.6 4.6A18 18 0 0 0 15.1 3.2l-.2.4a16.7 16.7 0 0 1 4 1.3 15.1 15.1 0 0 0-12 0 16.7 16.7 0 0 1 4-1.3l-.2-.4A18 18 0 0 0 4.4 4.6 18.9 18.9 0 0 0 1.2 17.2 18.1 18.1 0 0 0 6.7 20l.4-.6a11.9 11.9 0 0 1-1.9-.9l.5-.4a12.9 12.9 0 0 0 10.6 0l.5.4a11.9 11.9 0 0 1-1.9.9l.4.6a18 18 0 0 0 5.5-2.8 18.9 18.9 0 0 0-3.2-12.6ZM8.4 14.6c-.9 0-1.6-.8-1.6-1.8s.7-1.8 1.6-1.8 1.6.8 1.6 1.8-.7 1.8-1.6 1.8Zm7.2 0c-.9 0-1.6-.8-1.6-1.8s.7-1.8 1.6-1.8 1.6.8 1.6 1.8-.7 1.8-1.6 1.8Z",
  },
  {
    name: "GitHub",
    href: "https://github.com/decentraland",
    path: "M12 2a10 10 0 0 0-3.2 19.5c.5.1.7-.2.7-.5v-1.7c-2.8.6-3.4-1.4-3.4-1.4-.4-1.2-1.1-1.5-1.1-1.5-.9-.6.1-.6.1-.6 1 .1 1.5 1 1.5 1 .9 1.6 2.4 1.1 3 .9.1-.7.4-1.1.6-1.4-2.2-.2-4.6-1.1-4.6-5a3.9 3.9 0 0 1 1-2.7 3.6 3.6 0 0 1 .1-2.7s.9-.3 2.8 1a9.6 9.6 0 0 1 5 0c1.9-1.3 2.8-1 2.8-1a3.6 3.6 0 0 1 .1 2.7 3.9 3.9 0 0 1 1 2.7c0 3.9-2.3 4.7-4.6 5 .4.3.7.9.7 1.8v2.7c0 .3.2.6.7.5A10 10 0 0 0 12 2Z",
  },
  {
    name: "X",
    href: "https://x.com/decentraland",
    path: "M17.5 3h2.7l-5.9 6.7L21.3 21h-5.4l-4.3-5.6L6.7 21H4l6.3-7.2L3 3h5.5l3.9 5.1L17.5 3Zm-1 16.2h1.5L7.6 4.7H6l10.5 14.5Z",
  },
  {
    name: "Instagram",
    href: "https://instagram.com/decentraland_foundation/",
    path: "M12 2.2c3.2 0 3.6 0 4.9.1 1.2.1 1.8.3 2.2.4.6.2 1 .5 1.4.9.4.4.7.8.9 1.4.2.5.4 1.1.4 2.3.1 1.3.1 1.7.1 4.9s0 3.6-.1 4.9c-.1 1.2-.3 1.8-.4 2.2-.2.6-.5 1-.9 1.4-.4.4-.8.7-1.4.9-.5.2-1.1.4-2.3.4-1.3.1-1.7.1-4.9.1s-3.6 0-4.9-.1c-1.2-.1-1.8-.3-2.2-.4-.6-.2-1-.5-1.4-.9-.4-.4-.7-.8-.9-1.4-.2-.5-.4-1.1-.4-2.3-.1-1.3-.1-1.7-.1-4.9s0-3.6.1-4.9c.1-1.2.3-1.8.4-2.2.2-.6.5-1 .9-1.4.4-.4.8-.7 1.4-.9.5-.2 1.1-.4 2.3-.4C8.4 2.2 8.8 2.2 12 2.2Zm0 3.3a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13Zm0 10.7a4.2 4.2 0 1 1 0-8.4 4.2 4.2 0 0 1 0 8.4Zm6.7-11a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Z",
  },
  {
    name: "YouTube",
    href: "https://youtube.com/@decentraland_foundation",
    path: "M23 7.5a3 3 0 0 0-2.1-2.1C19 4.9 12 4.9 12 4.9s-7 0-8.9.5A3 3 0 0 0 1 7.5 31 31 0 0 0 .6 12 31 31 0 0 0 1 16.5a3 3 0 0 0 2.1 2.1c1.9.5 8.9.5 8.9.5s7 0 8.9-.5a3 3 0 0 0 2.1-2.1A31 31 0 0 0 23.4 12 31 31 0 0 0 23 7.5ZM9.8 15.3V8.7l5.7 3.3-5.7 3.3Z",
  },
  {
    name: "TikTok",
    href: "https://tiktok.com/@decentraland_fdn",
    path: "M19.59 6.69a4.83 4.83 0 0 1-3.77-4.25V2h-3.45v13.67a2.89 2.89 0 0 1-5.2 1.74 2.89 2.89 0 0 1 2.31-4.64c.3 0 .59.04.86.13V9.4a6.33 6.33 0 0 0-1-.05A6.34 6.34 0 0 0 5.6 20.95a6.34 6.34 0 0 0 10.86-4.43V9.01a8.16 8.16 0 0 0 4.77 1.52V7.1a4.85 4.85 0 0 1-1.64-.41Z",
  },
  {
    name: "LinkedIn",
    href: "https://linkedin.com/company/decentralandorg",
    path: "M20.45 20.45h-3.56v-5.57c0-1.33-.02-3.04-1.85-3.04-1.85 0-2.13 1.45-2.13 2.94v5.67H9.35V9h3.42v1.56h.05c.48-.9 1.64-1.85 3.37-1.85 3.6 0 4.27 2.37 4.27 5.46v6.28ZM5.34 7.43a2.06 2.06 0 1 1 0-4.13 2.06 2.06 0 0 1 0 4.13ZM7.12 20.45H3.55V9h3.57v11.45ZM22.22 0H1.77C.79 0 0 .77 0 1.73v20.54C0 23.22.79 24 1.77 24h20.45c.98 0 1.78-.78 1.78-1.73V1.73C24 .77 23.2 0 22.22 0Z",
  },
];

type SitesChromeProps = {
  children?: ReactNode;
  active?: SitesNavId;
  signedIn?: boolean;
  mana?: string;
  account?: string;
  onSignIn?: () => void;
  overlayNav?: boolean;
  hideNav?: boolean;
};

type NewsletterState =
  | { phase: "idle" }
  | { phase: "sending" }
  | { phase: "ok" }
  | { phase: "error"; message: string };

export default function SitesChrome({
  children,
  active = "whatson",
  signedIn,
  account,
  onSignIn,
  overlayNav = false,
  hideNav = false,
}: SitesChromeProps) {
  const pageId = useId();
  const [openCols, setOpenCols] = useState<Record<string, boolean>>({});
  const toggleCol = (title: string) => setOpenCols((o) => ({ ...o, [title]: !o[title] }));
  const auth = useChromeAuth();
  const isIn = signedIn ?? auth.signedIn;
  const acct = account ?? auth.account;
  const signIn = onSignIn ?? auth.onSignIn;

  const [email, setEmail] = useState("");
  const [news, setNews] = useState<NewsletterState>({ phase: "idle" });
  const submitNewsletter = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const value = email.trim();
    if (!EMAIL_RE.test(value)) {
      setNews({ phase: "error", message: "Enter a valid email address." });
      return;
    }
    setNews({ phase: "sending" });
    fetch("/v1/newsletter", {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({ email: value, source: siteHost() }),
    })
      .then((res) => {
        if (!res.ok) throw new Error(`newsletter returned ${res.status}`);
        setNews({ phase: "ok" });
        setEmail("");
      })
      .catch(() => {
        setNews({ phase: "error", message: "Something went wrong. Please try again." });
      });
  };

  const [lang, setLang] = useState<Language>(DEFAULT_LANG);
  const [langOpen, setLangOpen] = useState(false);
  const langRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    try {
      const saved = window.localStorage.getItem(LANG_STORAGE_KEY);
      const found = LANGUAGES.find((l) => l.code === saved);
      if (found) {
        setLang(found);
        document.documentElement.lang = found.code;
      }
    } catch {
    }
  }, []);

  useEffect(() => {
    if (!langOpen) return;
    const onDown = (e: MouseEvent) => {
      if (langRef.current && !langRef.current.contains(e.target as Node)) setLangOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setLangOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [langOpen]);

  const pickLang = (l: Language) => {
    setLang(l);
    setLangOpen(false);
    try {
      window.localStorage.setItem(LANG_STORAGE_KEY, l.code);
      document.cookie = `lang=${l.code}; path=/; max-age=31536000; samesite=lax`;
    } catch {
    }
    document.documentElement.lang = l.code;
  };

  return (
    <div className={"st ui2" + (overlayNav ? " st--overlay" : "")} data-label="Decentraland">
      <a className="st__skip" href={"#" + pageId}>Skip to content</a>
      {!hideNav && (
        <DclTopBar
          variant="sites"
          active={ACTIVE_MAP[active]}
          signedIn={isIn}
          account={acct}
          onSignIn={signIn}
          fetchNotifications={auth.fetchNotifications}
          transparent={overlayNav}
        />
      )}

      <div className="st__page" id={pageId} tabIndex={-1}>{children}</div>

      <footer className="st__footer">
        <div className="st__footin">
          <div className="st__footbrand">
            <a className="st__footlogo" href="/" aria-label="Decentraland Home">
              <span className="st__wordmark">Decentraland</span>
            </a>
            <div className="st__news">
              <p className="st__newstitle">Get the weekly highlights in your inbox</p>
              <form className="st__newsform" onSubmit={submitNewsletter} noValidate>
                <input
                  className="st__newsinput"
                  type="email"
                  placeholder="Enter Your Email"
                  aria-label="Email"
                  value={email}
                  onChange={(e) => {
                    setEmail(e.target.value);
                    if (news.phase !== "idle") setNews({ phase: "idle" });
                  }}
                  disabled={news.phase === "sending"}
                />
                <button type="submit" className="st__newsbtn" disabled={news.phase === "sending"}>
                  {news.phase === "sending" ? "Sending\u{2026}" : "Subscribe"}
                </button>
              </form>
              {news.phase === "ok" && (
                <p className="st__newsnote st__newsnote--ok" role="status">Subscribed!</p>
              )}
              {news.phase === "error" && (
                <p className="st__newsnote st__newsnote--err" role="alert">{news.message}</p>
              )}
            </div>
            <div className="st__connect">
              <div className="st__foottitle">Connect</div>
              <div className="st__socials">
                {SOCIALS.map((s) => (
                  <a key={s.name} href={s.href} className="st__social" aria-label={s.name} target="_blank" rel="noopener noreferrer">
                    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
                      <path d={s.path} fill="currentColor" />
                    </svg>
                  </a>
                ))}
              </div>
            </div>
          </div>

          <div className="st__footcols">
            <span className="st__footmenu">Menu</span>
            {FOOTER_COLS.map((col) => (
              <div key={col.title} className={"st__footcol" + (openCols[col.title] ? " is-open" : "")}>
                <div className="st__foottitle" onClick={() => toggleCol(col.title)}>
                  {col.title}
                  <Caret className="st__footcaret" open={!!openCols[col.title]} />
                </div>
                <ul className="st__footlist">
                  {col.links.map((link) => (
                    <li key={link.label}>
                      <a href={link.href} className="st__footlink">
                        {link.label}
                      </a>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </div>

        <div className="st__footbar">
          <div className="st__footbarleft">
            <div className="st__langwrap" ref={langRef}>
              <button
                type="button"
                className="st__lang"
                aria-label="Change language"
                aria-haspopup="listbox"
                aria-expanded={langOpen}
                onClick={() => setLangOpen((o) => !o)}
              >
                <img className="st__flag" src={flagUrl(lang.flag)} alt="" width="18" height="18" />
                {lang.label}
                <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                  <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" fill="none" />
                </svg>
              </button>
              {langOpen && (
                <div className="st__langmenu" role="listbox" aria-label="Language">
                  {LANGUAGES.map((l) => (
                    <button
                      key={l.code}
                      type="button"
                      role="option"
                      aria-selected={l.code === lang.code}
                      className={"st__langopt" + (l.code === lang.code ? " is-active" : "")}
                      onClick={() => pickLang(l)}
                    >
                      <img className="st__flag" src={flagUrl(l.flag)} alt="" width="18" height="18" />
                      {l.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
            <div className="st__legal">
              {LEGAL.map((l) => (
                <a key={l.label} href={l.href} className="st__legallink">
                  {l.label}
                </a>
              ))}
            </div>
          </div>
          <span className="st__copy">&copy; {new Date().getFullYear()} Decentraland</span>
        </div>
      </footer>
    </div>
  );
}

export function SitesChromeMaybe({
  chrome = true,
  children,
  ...rest
}: SitesChromeProps & { chrome?: boolean }) {
  if (!chrome) return <>{children}</>;
  return <SitesChrome {...rest}>{children}</SitesChrome>;
}
