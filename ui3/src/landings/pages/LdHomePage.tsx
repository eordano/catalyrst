import type { ComponentProps, MouseEvent, ReactNode } from "react";

import SitesHome from "../../web/pages/SitesHome";
import "./ldhomepage.css";

type SitesHomeProps = ComponentProps<typeof SitesHome>;

export type LdHomeCta = { label: string; href: string };

export type LdHomeHero = {
  kicker: string;
  title: string;
  subtitle: string;
  downloads: string | null;
  downloadsLabel: string;
  cta: { desktop: LdHomeCta; epic: LdHomeCta };
  platforms: { id: string; label: string; href: string }[];
};

export type LdHomeRailItem = {
  id: string;
  title: string;
  href: string;
  category?: string;
  live?: boolean;
  when?: string;
};

export type LdHomeRail = {
  id: string;
  title: string;
  kind: string;
  viewAll?: LdHomeCta | null;
  items: LdHomeRailItem[];
};

type NavigateFn = (href: string, e: MouseEvent<HTMLAnchorElement>) => void;

type LdHomePageProps = {
  hero: LdHomeHero;
  rails?: LdHomeRail[];
  comeHangOutTitle: string;
  live?: boolean;
  events?: SitesHomeProps["events"];
  hotspots?: SitesHomeProps["hotspots"];
  rituals?: SitesHomeProps["rituals"];
  shopSlot?: ReactNode;
  onDownloadClick?: (store: string, placement: string) => void;
  onRailClick?: (rail: string, target: string) => void;
  onNavigate?: NavigateFn;
  railRef?: (id: string) => (node: HTMLElement | null) => void;
};

export default function LdHomePage({
  hero,
  rails = [],
  comeHangOutTitle,
  live = false,
  events = undefined,
  hotspots = undefined,
  rituals = undefined,
  shopSlot = undefined,
  onDownloadClick = undefined,
  onRailClick = undefined,
  onNavigate = undefined,
  railRef = undefined,
}: LdHomePageProps) {
  return (
    <div className="landings-home">
      <SitesHome
        downloads={hero.downloads}
        events={events}
        hotspots={hotspots}
        rituals={rituals}
      />

      <section
        id="hero"
        aria-label="Jump in to Decentraland"
        className="landings-home__cta"
      >
        <h2 className="landings-home__kicker">{hero.kicker}</h2>
        <p className="landings-home__title">{hero.title}</p>
        <p className="landings-home__sub">{hero.subtitle}</p>
        <div id="download-cta" className="landings-home__btns">
          <a
            className="landings-home__btn landings-home__btn--primary"
            href={hero.cta.desktop.href}
            onClick={() => onDownloadClick?.("desktop", "hero")}
          >
            {hero.cta.desktop.label}
          </a>
          <a
            className="landings-home__btn"
            href={hero.cta.epic.href}
            target="_blank"
            rel="noopener noreferrer"
            onClick={() => onDownloadClick?.("epic", "hero")}
          >
            {hero.cta.epic.label}
          </a>
          {hero.platforms.map((p) => (
            <a
              key={p.id}
              className="landings-home__btn landings-home__btn--ghost"
              href={p.href}
              onClick={() => onDownloadClick?.(p.id, "hero")}
            >
              {p.label}
            </a>
          ))}
        </div>
        {hero.downloads ? (
          <p className="landings-home__downloads">
            {hero.downloads} {hero.downloadsLabel}
            {live ? " \u{B7} live events" : ""}
          </p>
        ) : null}
      </section>

      {shopSlot}

      <section
        id="feature-rails"
        aria-label="Explore Decentraland"
        className="landings-home__rails"
      >
        {rails.map((rail) => (
          <div
            key={rail.id}
            ref={railRef?.(rail.id)}
            className="landings-home__rail"
            aria-label={rail.title}
          >
            <div className="landings-home__railhead">
              <h3 className="landings-home__railtitle">{rail.title}</h3>
              {rail.viewAll ? (
                <a
                  className="landings-home__viewall"
                  href={rail.viewAll.href}
                  onClick={(e) => {
                    onRailClick?.(rail.id, rail.viewAll!.href);
                    onNavigate?.(rail.viewAll!.href, e);
                  }}
                >
                  {rail.viewAll.label}
                </a>
              ) : null}
            </div>
            <RailLinks rail={rail} onItemClick={onRailClick} onNavigate={onNavigate} />
          </div>
        ))}
      </section>

      <section className="landings-home__banner" aria-label={comeHangOutTitle}>
        <h2 className="landings-home__title">{comeHangOutTitle}</h2>
        <div className="landings-home__btns">
          <a
            className="landings-home__btn landings-home__btn--primary"
            href={hero.cta.desktop.href}
            onClick={() => onDownloadClick?.("desktop", "come-hang-out")}
          >
            {hero.cta.desktop.label}
          </a>
          <a
            className="landings-home__btn"
            href={hero.cta.epic.href}
            target="_blank"
            rel="noopener noreferrer"
            onClick={() => onDownloadClick?.("epic", "come-hang-out")}
          >
            {hero.cta.epic.label}
          </a>
        </div>
      </section>
    </div>
  );
}

function RailLinks({
  rail,
  onItemClick,
  onNavigate,
}: {
  rail: LdHomeRail;
  onItemClick?: (rail: string, target: string) => void;
  onNavigate?: NavigateFn;
}) {
  if (rail.kind === "events") {
    if (!rail.items.length) return null;
    return (
      <nav className="landings-home__raillinks" aria-label={rail.title}>
        {rail.items.map((ev) => (
          <a
            key={ev.id}
            className="landings-home__raillink"
            href={ev.href}
            onClick={(e) => {
              onItemClick?.(rail.id, ev.id);
              onNavigate?.(ev.href, e);
            }}
          >
            <span className="landings-home__railcat">{ev.category}</span>
            {ev.title}
            {ev.live ? " \u{2014} LIVE" : ` \u{B7} ${ev.when}`}
          </a>
        ))}
      </nav>
    );
  }

  if (rail.kind === "rituals") {
    if (!rail.items.length) return null;
    return (
      <nav className="landings-home__raillinks" aria-label={rail.title}>
        {rail.items.map((r) => (
          <a
            key={r.id}
            className="landings-home__raillink"
            href={r.href}
            onClick={(e) => {
              onItemClick?.(rail.id, r.id);
              onNavigate?.(r.href, e);
            }}
          >
            {r.title}
          </a>
        ))}
      </nav>
    );
  }

  return (
    <nav className="landings-home__raillinks" aria-label={rail.title}>
      <a
        className="landings-home__raillink"
        href="/play/"
        onClick={() => onItemClick?.(rail.id, "hang-out-now")}
      >
        Hang Out Now
      </a>
    </nav>
  );
}
