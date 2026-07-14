import { siteUrl } from "../../data/site";
import type { CSSProperties } from "react";
import SitesChrome from "../frames/SitesChrome";
import { asset } from "../../asset";
import "./siteshome.css";

type StoreLink = { id: string; label: string; href: string };

// Explorer-first: phones open the web explorer too — no store detour.
const STORE_LINKS: StoreLink[] = [
  { id: "ios", label: "iOS", href: "/play/" },
  { id: "android", label: "Android", href: "/play/" },
];

type HomeEvent = {
  id: string;
  cat?: string;
  category?: string;
  title: string;
  when?: string;
  grad?: string;
  image?: string | null;
  live?: boolean;
};

const EVENTS: HomeEvent[] = [
  { id: "e1", cat: "MUSIC", title: "Sunset Synthwave — Live DJ Set on the Pier", when: "Today · 8:00 PM", grad: "linear-gradient(135deg, #ff2d55 0%, #982de2 70%)", live: true },
  { id: "e2", cat: "COMMUNITY", title: "Metaverse Fashion Week — Opening Night", when: "Fri · 6:00 PM", grad: "linear-gradient(135deg, #ff4bed 0%, #6a1fb0 70%)" },
  { id: "e3", cat: "GAMES", title: "Genesis Plaza Trivia Night", when: "Sat · 7:00 PM", grad: "linear-gradient(135deg, #1f8a70 0%, #438fff 70%)" },
];

type HomeHotspot = {
  id: string;
  title: string;
  online?: number | null;
  image?: string | null;
  grad?: string;
  href?: string;
};

const HOTSPOTS: HomeHotspot[] = [
  { id: "genesis-plaza", title: "Genesis Plaza", grad: "linear-gradient(135deg, #ff743a 0%, #982de2 60%, #1a0a2e 100%)", href: siteUrl("/play/?position=-3%2C-2") },
  { id: "wondermine", title: "WonderMine Crafting Game", grad: "linear-gradient(135deg, #438fff 0%, #ff2d55 60%, #220040 100%)", href: siteUrl("/play/?position=-29%2C55") },
];

type HomeRitual = {
  id: string;
  title: string;
  day?: string | null;
  image?: string | null;
  grad?: string;
  href?: string;
};

const RITUALS: HomeRitual[] = [
  { id: "career-mondays", title: "Career Mondays", grad: "linear-gradient(135deg, #438fff, #2f004d)" },
  { id: "takeover-tuesdays", title: "Takeover Tuesdays", grad: "linear-gradient(135deg, #ff2d55, #350447)" },
  { id: "watch-party-wednesdays", title: "Watch Party Wednesdays", grad: "linear-gradient(135deg, #b05cff, #220040)" },
  { id: "trivia-thursdays", title: "Trivia Thursdays", grad: "linear-gradient(135deg, #1f8a70, #06231c)" },
  { id: "play-with-friends-fridays", title: "Play with Friends Fridays", grad: "linear-gradient(135deg, #ff743a, #3a1500)" },
];

function bgStyle(image?: string | null, grad?: string): CSSProperties {
  return image
    ? {
        backgroundImage: `url("${image}")`,
        backgroundSize: "cover",
        backgroundPosition: "center",
      }
    : { background: grad };
}

const VerifiedMark = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M12 2 14.9 4.6 18.8 4.2 19.8 8 23 10.3 21.2 13.6 22.4 17.3 18.6 18.1 16.8 21.5 13 20.1 9.2 21.5 7.4 18.1 3.6 17.3 4.8 13.6 3 10.3 6.2 8 7.2 4.2 11.1 4.6 12 2Z" fill="var(--brand)" />
    <path d="m8.5 12 2.4 2.4 4.6-4.8" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" fill="none" />
  </svg>
);

const DesktopMark = () => (
  <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
    <rect x="2.5" y="4" width="19" height="13" rx="2" stroke="currentColor" strokeWidth="1.7" fill="none" />
    <path d="M9 20h6M12 17v3" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
  </svg>
);

const EpicMark = () => (
  <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
    <rect x="4" y="2.5" width="16" height="19" rx="3" fill="currentColor" />
    <path d="M9 7h6M9 12h5M9 17h6" stroke="#fff" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);

const ArrowMark = () => (
  <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
    <path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" fill="none" />
  </svg>
);
function DownloadCta({ downloads }: { downloads: string | null }) {
  return (
    <>
      <div className="sh__cta">
        <a className="sh__btn sh__btn--primary" href="/play/">
          <DesktopMark />
          Jump in — no download
        </a>
        <a className="sh__btn sh__btn--epic" href="/play/">
          <EpicMark />
          Play in your browser
        </a>
      </div>
      <div className="sh__meta">
        {downloads ? (
          <>
            <span className="sh__downloads"><VerifiedMark />{downloads} downloads</span>
            <span className="sh__sep" aria-hidden="true" />
          </>
        ) : null}
        <div className="sh__platforms">
          {STORE_LINKS.map((p) => (
            <a key={p.id} className="sh__platform" href={p.href} aria-label={`Get the app on ${p.label}`}>{p.label}</a>
          ))}
        </div>
      </div>
    </>
  );
}

type SitesHomeProps = {
  downloads?: string | null;
  events?: HomeEvent[];
  hotspots?: HomeHotspot[];
  rituals?: HomeRitual[];
};

export default function SitesHome({
  downloads = null,
  events,
  hotspots,
  rituals,
}: SitesHomeProps) {
  const eventCards = events && events.length > 0 ? events : EVENTS;
  const hotspotCards = hotspots && hotspots.length > 0 ? hotspots : HOTSPOTS;
  const ritualCards = rituals && rituals.length > 0 ? rituals : RITUALS;
  return (
    <SitesChrome active="play" overlayNav>
      <div className="sh">
        <section className="sh__hero">
          <img className="sh__heroimg" src={asset("assets/home-hero.webp")} alt="" aria-hidden="true" />
          <div className="sh__heroinner">
            <span className="sh__kicker">Decentraland now on mobile</span>
            <h1 className="sh__title">Hang Out From Anywhere</h1>
            <p className="sh__sub">Close the Feed. Come Hang Out.</p>
            <DownloadCta downloads={downloads} />
          </div>
        </section>

        <section className="sh__sec sh__whatson">
          <div className="sh__sechead">
            <h2 className="sh__h2">Jump Into What's Happening</h2>
            <a className="sh__viewall" href="/whats-on">
              View All <ArrowMark />
            </a>
          </div>
          <div className="sh__eventgrid">
            {eventCards.map((ev) => (
              <article className="sh__event" key={ev.id}>
                <div
                  className="sh__eventimg"
                  style={
                    ev.image
                      ? {
                          backgroundImage: `url("${ev.image}")`,
                          backgroundSize: "cover",
                          backgroundPosition: "center",
                        }
                      : { background: ev.grad }
                  }
                >
                  {ev.live ? <span className="sh__eventlive">● LIVE</span> : null}
                </div>
                <div className="sh__eventmeta">
                  <span className="sh__eventcat">{ev.cat ?? ev.category}</span>
                  <h3 className="sh__eventtitle">{ev.title}</h3>
                  <span className="sh__eventwhen">{ev.when}</span>
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="sh__sec sh__vibe">
          <h2 className="sh__h2">Catch the Vibe</h2>
          <div className="sh__viberow">
            {hotspotCards.map((v) => (
              <article className="sh__vibecard" key={v.id}>
                <div className="sh__vibeimg" style={bgStyle(v.image, v.grad)}>
                  {v.online != null && v.online > 0 ? (
                    <span className="sh__online">● {v.online} ONLINE</span>
                  ) : null}
                </div>
                <div className="sh__vibefoot">
                  <span className="sh__vibeuser">{v.title}</span>
                  <a className="sh__hangout" href={v.href ?? siteUrl("/play/")}>HANG OUT NOW</a>
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="sh__sec sh__rituals">
          <h2 className="sh__h2">Your Weekly Rituals</h2>
          <div className="sh__ritualrow">
            {ritualCards.map((r) => (
              <a className="sh__ritual" key={r.id} href={r.href ?? "/whats-on"} style={bgStyle(r.image, r.grad)}>
                {r.day ? <span className="sh__ritualwhen">{r.day}</span> : null}
                <span className="sh__ritualday">{r.title}</span>
              </a>
            ))}
          </div>
        </section>

        <section className="sh__banner">
          <img className="sh__banimg" src={asset("assets/come-hang-out.webp")} alt="" aria-hidden="true" />
          <div className="sh__bancontent">
            <h2 className="sh__bantitle">Come Hang Out</h2>
            <DownloadCta downloads={downloads} />
          </div>
        </section>
      </div>
    </SitesChrome>
  );
}
