import type { CSSProperties, ReactNode } from "react";
import { createContext, useContext, useEffect, useState } from "react";
import { asset } from "../../asset";
import ProfileWidget from "../components/ProfileWidget";
import { Close } from "../../atoms/icons";
import "./explorechrome.css";

const ChromeNestedContext = createContext(false);

// Mirrors upstream MainMenuShell's openShells guard: counts how many ExploreChrome
// instances are mounted so a route switch (unmount+remount somewhere in the tree,
// e.g. Suspense/StrictMode) doesn't replay the entrance fade — only a truly fresh
// open (nothing else mounted yet) animates in.
let openChromes = 0;

export type TabId =
  | "events"
  | "places"
  | "communities"
  | "map"
  | "backpack"
  | "gallery"
  | "settings";

type ExploreTab = {
  id: TabId;
  label: string;
  hint: string;
  badge?: boolean;
  count?: number;
  to: string;
};

export const EXPLORE_TABS: ExploreTab[] = [
  { id: "events", label: "Events", hint: "X", badge: true, to: "Explorer/Pages/Events" },
  { id: "places", label: "Places", hint: "Z", to: "Explorer/Pages/Places" },
  { id: "communities", label: "Communities", hint: "O", count: 0, to: "Explorer/Pages/Communities" },
  { id: "map", label: "Map", hint: "M", to: "Explorer/Pages/Map" },
  { id: "backpack", label: "Backpack", hint: "I", to: "Explorer/Pages/Backpack" },
  { id: "gallery", label: "Gallery", hint: "K", to: "Explorer/Pages/Reel" },
  { id: "settings", label: "Settings", hint: "P", to: "Explorer/Pages/Settings" },
];

const ICONS: Record<string, ReactNode> = {
  events: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect x="3" y="4.5" width="18" height="16" rx="2.5" />
      <path d="M3 9h18M8 2.5v4M16 2.5v4" />
    </svg>
  ),
  places: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 21s7-6.2 7-11a7 7 0 1 0-14 0c0 4.8 7 11 7 11Z" />
      <circle cx="12" cy="10" r="2.6" />
    </svg>
  ),
  communities: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="9" cy="8" r="3.2" />
      <path d="M3 20c0-3.3 2.7-5.5 6-5.5s6 2.2 6 5.5" />
      <path d="M16 5.2a3.2 3.2 0 0 1 0 5.6M17 14.7c2.4.5 4 2.4 4 5.3" />
    </svg>
  ),
  map: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M9 4 3.5 6.2v13.3L9 17.3l6 2.4 5.5-2.2V4.2L15 6.4 9 4Z" />
      <path d="M9 4v13.3M15 6.4v13.3" />
    </svg>
  ),
  backpack: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M5 9.5A4.5 4.5 0 0 1 9.5 5h5A4.5 4.5 0 0 1 19 9.5V20a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V9.5Z" />
      <path d="M9 5a3 3 0 0 1 6 0M8 13h8" />
    </svg>
  ),
  gallery: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect x="3" y="6" width="18" height="14" rx="2.5" />
      <path d="M8 6 9.5 3.5h5L16 6" />
      <circle cx="12" cy="13" r="3.4" />
    </svg>
  ),
  settings: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="3.2" />
      <path d="M12 2.5v3M12 18.5v3M21.5 12h-3M5.5 12h-3M18.7 5.3l-2.1 2.1M7.4 16.6l-2.1 2.1M18.7 18.7l-2.1-2.1M7.4 7.4 5.3 5.3" />
    </svg>
  ),
};

type ExploreChromeProps = {
  active?: TabId;
  children?: ReactNode;
  onTab?: (id: TabId) => void;
  user?: string;
  onClose?: () => void;
  avatarSrc?: string | null;
  tag?: string;
  wallet?: string;
  address?: string;
  isGuest?: boolean;
  profileOpen?: boolean;
  onProfileToggle?: () => void;
  onSignOut?: () => void;
};

export default function ExploreChrome({
  active,
  children,
  onTab,
  user = "Guest",
  onClose = () => {},
  avatarSrc,
  tag,
  wallet,
  address,
  isGuest,
  profileOpen = false,
  onProfileToggle,
  onSignOut,
}: ExploreChromeProps) {
  const nested = useContext(ChromeNestedContext);
  // A fresh open (nothing else already mounted) fades in; a remount while another
  // instance is still up (route swap under the same layout) doesn't replay it.
  const [animate] = useState(() => !nested && openChromes === 0);
  useEffect(() => {
    if (nested) return undefined;
    openChromes++;
    return () => {
      openChromes--;
    };
  }, [nested]);
  if (nested) return <>{children}</>;
  return (
    <ChromeNestedContext.Provider value={true}>
    <div className={"xc" + (active ? "" : " xc--hud") + (animate ? " xc--enter" : "")} role="dialog" aria-label="Explore">
      <a className="xc__skip" href="#xc-page">Skip to content</a>
      <header className="xc__nav">
        <div className="xc__brand">
          <img src={asset("assets/dcl-logo.png")} alt="" />
          <span>Decentraland</span>
        </div>

        <nav className="xc__tabs" aria-label="Explore sections">
          {EXPLORE_TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className={"xc__tab" + (t.id === active ? " is-active" : "")}
              aria-current={t.id === active ? "page" : undefined}
              data-sb-linkto={t.to}
              onClick={() => onTab?.(t.id)}
            >
              <span className="xc__ticon">
                {ICONS[t.id]}
                {t.badge ? <span className="xc__dot" /> : null}
              </span>
              <span className="xc__tlabel">
                {t.label}
                <em className="xc__hint">
                  {t.count != null ? `[${t.count}]` : `[${t.hint}]`}
                </em>
              </span>
            </button>
          ))}
        </nav>

        <div className="xc__right">
          <button
            type="button"
            className="xc__user"
            aria-haspopup="dialog"
            aria-expanded={profileOpen}
            onClick={onProfileToggle}
          >
            <span className="xc__avatar u-avatar" style={{ "--sz": "28px", "--hue": 300 } as CSSProperties} />
            <span className="xc__uname">{user}</span>
          </button>
          <button
            type="button"
            className="xc__close"
            aria-label="Back to world"
            title="Back to world (Esc)"
            onClick={onClose}
          >
            <Close strokeWidth={2.2} />
          </button>
        </div>
      </header>

      <div className="xc__profileflyout">
        <ProfileWidget
          open={profileOpen}
          name={user}
          tag={tag}
          wallet={wallet}
          address={address}
          avatarSrc={avatarSrc}
          isGuest={isGuest}
          onClose={onProfileToggle}
          onSignOut={onSignOut}
          anchor="topbar"
        />
      </div>

      <div className="xc__body" id="xc-page" tabIndex={-1}>{children}</div>
    </div>
    </ChromeNestedContext.Provider>
  );
}
