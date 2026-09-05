import type { ReactNode } from "react";
import type { MobileOrientation, MobileTab, MobileTabGlyph } from "./types";
import "./mobiletabbar.css";

export const MOBILE_TABS: MobileTab[] = [
  { id: "places", label: "Places", glyph: "places", to: "Explorer/Pages/Places" },
  { id: "events", label: "Events", glyph: "events", to: "Explorer/Pages/Events" },
  { id: "map", label: "Map", glyph: "map", to: "Explorer/Pages/Map" },
  { id: "backpack", label: "Backpack", glyph: "backpack", to: "Explorer/Pages/Backpack" },
  { id: "settings", label: "Settings", glyph: "settings", to: "Explorer/Pages/Settings" },
];

const GLYPHS: Record<MobileTabGlyph, ReactNode> = {
  places: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
      strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 21s7-6.2 7-11a7 7 0 1 0-14 0c0 4.8 7 11 7 11Z" />
      <circle cx="12" cy="10" r="2.6" />
    </svg>
  ),
  events: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
      strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <rect x="3" y="4.5" width="18" height="16" rx="2.5" />
      <path d="M3 9h18M8 2.5v4M16 2.5v4" />
    </svg>
  ),
  map: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
      strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M9 4 3.5 6.2v13.3L9 17.3l6 2.4 5.5-2.2V4.2L15 6.4 9 4Z" />
      <path d="M9 4v13.3M15 6.4v13.3" />
    </svg>
  ),
  backpack: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
      strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M5 9.5A4.5 4.5 0 0 1 9.5 5h5A4.5 4.5 0 0 1 19 9.5V20a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V9.5Z" />
      <path d="M9 5a3 3 0 0 1 6 0M8 13h8" />
    </svg>
  ),
  settings: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
      strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="3.2" />
      <path d="M12 2.5v3M12 18.5v3M21.5 12h-3M5.5 12h-3M18.7 5.3l-2.1 2.1M7.4 16.6l-2.1 2.1M18.7 18.7l-2.1-2.1M7.4 7.4 5.3 5.3" />
    </svg>
  ),
  profile: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
      strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="8.4" r="3.6" />
      <path d="M4.6 20c.6-3.6 3.7-5.6 7.4-5.6s6.8 2 7.4 5.6" />
    </svg>
  ),
};

type MobileTabBarProps = {
  orientation?: MobileOrientation;
  landscape?: "hidden" | "pill";
  tabs?: MobileTab[];
  active?: string;
  onTab?: (id: string) => void;
};

export default function MobileTabBar({
  orientation = "portrait",
  landscape = "hidden",
  tabs = MOBILE_TABS,
  active,
  onTab,
}: MobileTabBarProps) {
  if (orientation === "landscape" && landscape === "hidden") return null;
  if (tabs.length === 0) return null;
  return (
    <nav className="mtabs" data-orientation={orientation} aria-label="Explore sections">
      {tabs.map((t) => (
        <button
          key={t.id}
          type="button"
          className={"mtabs__tab" + (t.id === active ? " is-active" : "")}
          aria-label={t.label}
          aria-current={t.id === active ? "page" : undefined}
          data-sb-linkto={t.to}
          onClick={() => onTab?.(t.id)}
        >
          <span className="mtabs__ticon">
            {GLYPHS[t.glyph]}
            {t.badge ? <span className="mtabs__dot" /> : null}
          </span>
          <span className="mtabs__label">{t.label}</span>
        </button>
      ))}
    </nav>
  );
}
