import type { ReactNode } from "react";
import { CreatorHubChromeMaybe } from "../frames/CreatorHubChrome";
import { useChromeAuth } from "../../web/frames/chrome-auth";
import EmptyState from "../../components/EmptyState";
import Spinner from "../../atoms/Spinner";
import { asset } from "../../asset";
import "./creatorhubhome.css";

type LearnResource = { title: string; href: string; kind: "doc" | "video" };

const LEARN_RESOURCES: LearnResource[] = [
  { title: "Let's build the metaverse together", href: "https://docs.decentraland.org/creator/", kind: "doc" },
  { title: "Scene Editor About", href: "https://docs.decentraland.org/creator/scene-editor/get-started/about-editor", kind: "doc" },
  { title: "Development Workflow", href: "https://docs.decentraland.org/creator/scenes-sdk7/getting-started/dev-workflow", kind: "doc" },
  { title: "Product Updates", href: "https://www.youtube.com/playlist?list=PLAcRraQmr_GMJw77zKvN84LX_OLyn-lVz", kind: "video" },
  { title: "SDK Tutorials", href: "https://www.youtube.com/playlist?list=PLAcRraQmr_GP_K8WN7csnKnImK4R2TgMA", kind: "video" },
];

const LayersIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path d="M12 3 3 8l9 5 9-5-9-5Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
    <path d="M3 13l9 5 9-5M3 16l9 5 9-5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" opacity="0.7" />
  </svg>
);
const BookmarkIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path d="M6 3.5h12a1 1 0 0 1 1 1V21l-7-4.2L5 21V4.5a1 1 0 0 1 1-1Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
  </svg>
);
const VideoIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <rect x="3" y="5.5" width="13" height="13" rx="2.2" fill="none" stroke="currentColor" strokeWidth="1.6" />
    <path d="M16 10.5 21 8v8l-5-2.5v-3Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
  </svg>
);

const NewProjectIcon = () => (
  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
    <rect x="3" y="3" width="14" height="14" rx="2.4" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <path d="M10 7v6M7 10h6" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);
const DeployIcon = () => (
  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
    <path d="M10 13V4M6.5 7.5 10 4l3.5 3.5" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M4 14.5v1A1.5 1.5 0 0 0 5.5 17h9a1.5 1.5 0 0 0 1.5-1.5v-1" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const TemplatesIcon = () => (
  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
    <rect x="3" y="3" width="6" height="6" rx="1.3" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <rect x="11" y="3" width="6" height="6" rx="1.3" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <rect x="3" y="11" width="6" height="6" rx="1.3" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <rect x="11" y="11" width="6" height="6" rx="1.3" stroke="currentColor" strokeWidth="1.5" fill="none" />
  </svg>
);
const DesktopAppIcon = () => (
  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
    <rect x="2.5" y="3.5" width="15" height="10" rx="1.6" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <path d="M7.5 17h5M10 13.5V17" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M10 6v3.6M8.2 7.8 10 9.6l1.8-1.8" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const CompassArt = () => (
  <svg className="chh__exploreart" viewBox="0 0 24 24" aria-hidden="true">
    <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="1.6" />
    <path d="m15.2 8.8-1.7 4.7-4.7 1.7 1.7-4.7 4.7-1.7Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
  </svg>
);

const CollectionsIcon = () => (
  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
    <path d="M3 3h6l8 8-6 6-8-8V3Z" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinejoin="round" />
    <circle cx="6.4" cy="6.4" r="1.1" fill="currentColor" />
  </svg>
);
const WorldsIcon = () => (
  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
    <circle cx="10" cy="10" r="7" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <path d="M3 10h14M10 3c2.2 2.4 2.2 11.6 0 14M10 3c-2.2 2.4-2.2 11.6 0 14" stroke="currentColor" strokeWidth="1.4" fill="none" />
  </svg>
);
const MetricsIcon = () => (
  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
    <path d="M3 16.5h14" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" />
    <path d="M6 16.5V12M10 16.5V8.5M14 16.5V5" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" />
  </svg>
);
const LandIcon = () => (
  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
    <rect x="3" y="3" width="14" height="14" rx="1.3" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <path d="M10 3v14M3 10h14" stroke="currentColor" strokeWidth="1.5" fill="none" />
  </svg>
);
const NamesIcon = () => (
  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
    <path d="M10.3 3H16a1 1 0 0 1 1 1v5.7a1 1 0 0 1-.3.7l-6 6a1 1 0 0 1-1.4 0L3.6 11.7a1 1 0 0 1 0-1.4l6-6a1 1 0 0 1 .7-.3Z" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinejoin="round" />
    <circle cx="13.3" cy="6.7" r="1.1" fill="currentColor" />
  </svg>
);
const ShieldCheckIcon = () => (
  <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
    <path d="M10 2.5 16 4.7v4.3c0 3.4-2.4 6.5-6 8-3.6-1.5-6-4.6-6-8V4.7L10 2.5Z" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinejoin="round" />
    <path d="m7.3 9.8 1.9 1.9 3.5-3.6" stroke="currentColor" strokeWidth="1.5" fill="none" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const ManageArt = () => (
  <svg className="chh__exploreart" viewBox="0 0 24 24" aria-hidden="true">
    <path d="M4 7h9M16 7h4M4 12h4M11 12h9M4 17h12M19 17h1" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
    <circle cx="14.5" cy="7" r="2" fill="none" stroke="currentColor" strokeWidth="1.6" />
    <circle cx="9.5" cy="12" r="2" fill="none" stroke="currentColor" strokeWidth="1.6" />
    <circle cx="17.5" cy="17" r="2" fill="none" stroke="currentColor" strokeWidth="1.6" />
  </svg>
);

type ExploreLink = { title: string; href: string; icon: ReactNode };
type ManageLink = { title: string; href: string; icon: ReactNode; builder?: boolean; badge?: string };

const EXPLORE_LINKS: ExploreLink[] = [
  { title: "Your published scenes", href: "/creator-hub/my-scenes", icon: <LayersIcon /> },
  { title: "New scene", href: "/creator-hub/scene-editor?new=1&from=home", icon: <NewProjectIcon /> },
  { title: "Browse templates", href: "/create/templates", icon: <TemplatesIcon /> },
  { title: "Deploy a scene", href: "/creator-hub/deploy-world?from=home", icon: <DeployIcon /> },
  { title: "Get the desktop app", href: "/landings/creator-hub-download", icon: <DesktopAppIcon /> },
];

const MANAGE_LINKS: ManageLink[] = [
  { title: "Collections", href: "/create/wearables", icon: <CollectionsIcon /> },
  { title: "Worlds", href: "/creator-hub/manage", icon: <WorldsIcon /> },
  { title: "Metrics", href: "/creator-hub/metrics", icon: <MetricsIcon /> },
  { title: "Curate", href: "/create/curate", icon: <ShieldCheckIcon />, badge: "Committee" },
  { title: "Land", href: "/shop", icon: <LandIcon /> },
  { title: "Names", href: "/marketplace/names", icon: <NamesIcon /> },
];

const EditorArt = () => (
  <img className="chh__bannerart" src={asset("assets/ch-home-editor.png")} alt="" aria-hidden="true" />
);
const BookArt = () => (
  <img className="chh__bannerart" src={asset("assets/ch-home-book.png")} alt="" aria-hidden="true" />
);

type CardBannerProps = { art: ReactNode; title: string };

function CardBanner({ art, title }: CardBannerProps) {
  return (
    <div className="chh__banner">
      <span className="chh__bannerimg">{art}</span>
      <h2 className="chh__bannertitle">{title}</h2>
    </div>
  );
}

const ExternalGlyph = () => (
  <svg className="chh__itemext" viewBox="0 0 20 20" width="13" height="13" aria-hidden="true">
    <path d="M8 4.5H5.2a1 1 0 0 0-1 1V15a1 1 0 0 0 1 1h9.6a1 1 0 0 0 1-1v-2.8" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M11.5 4.5H15.5V8.5M15.5 4.5 9.5 10.5" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

type CardItemProps = {
  title: string;
  icon?: ReactNode;
  href?: string | null;
  external?: boolean;
  builder?: boolean;
  badge?: string;
};

function CardItem({ title, icon, href, external, builder, badge }: CardItemProps) {
  const inner = (
    <>
      <span className="chh__itemicon" aria-hidden="true">{icon}</span>
      <span className="chh__itemtitle u-truncate">{title}</span>
      {builder || external ? <ExternalGlyph /> : null}
      {badge ? <span className="chh__itembadge">{badge}</span> : null}
    </>
  );
  if (href != null) {
    const ext = external ? ({ target: "_blank", rel: "noreferrer" } as const) : {};
    const labelAttrs = builder
      ? { "aria-label": `${title} (opens Builder)`, title: `${title} (opens Builder)` }
      : external
      ? { "aria-label": `${title} (opens in new tab)`, title: `${title} (opens in new tab)` }
      : {};
    return (
      <a className="chh__item" href={href} {...ext} {...labelAttrs}>
        {inner}
      </a>
    );
  }
  return <span className="chh__item chh__item--inert" aria-disabled="true">{inner}</span>;
}

function shortAddress(addr: string): string {
  return addr && addr.length > 12 ? `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}` : addr;
}

type ChScene = { id: string; title: string; href?: string | null };

type ScenesCardProps = {
  scenes: ChScene[];
  scenesError?: boolean;
  rescoping?: boolean;
  signedIn?: boolean;
  onSignIn?: () => void;
  onScenes?: () => void;
  onStartBuilding?: () => void;
  onRetry?: () => void;
};

function PublishedScenesFooter() {
  return (
    <div className="chh__cardactions">
      <a
        className="chh__seeall"
        href="/creator-hub/my-scenes"
        aria-label="Find scenes you've already published"
      >
        Your published scenes
      </a>
    </div>
  );
}

function ScenesCard({ scenes, scenesError, rescoping, signedIn, onSignIn, onScenes, onStartBuilding, onRetry }: ScenesCardProps) {
  const empty = scenes.length === 0;
  return (
    <article className="chh__card">
      <CardBanner art={<EditorArt />} title="Scenes" />
      {scenesError ? (
        <div className="chh__cardbody chh__cardbody--centered">
          <p className="chh__outagetext" role="alert">
            Couldn&#x2019;t load your scenes just now &#x2014; the Builder data layer may be
            temporarily unavailable. Your work isn&#x2019;t lost.
          </p>
          {onRetry ? (
            <button type="button" className="chh__ghostbtn" onClick={onRetry}>Try again</button>
          ) : null}
        </div>
      ) : rescoping ? (
        <div
          className="chh__cardbody chh__cardbody--centered"
          role="status"
          aria-live="polite"
        >
          <Spinner size={36} />
          <p className="chh__loadingtext">Loading your scenes&#x2026;</p>
        </div>
      ) : empty && !signedIn ? (
        <>
          <div className="chh__cardbody chh__cardbody--centered">
            <EmptyState
              variant="inline"
              title="Sign in to see your scenes"
              subtitle="Your scenes will appear here once your wallet is connected."
              actions={[{ label: "Sign in", onClick: onSignIn }]}
            />
          </div>
          <PublishedScenesFooter />
        </>
      ) : empty ? (
        <>
          <div className="chh__cardbody chh__cardbody--centered">
            <EmptyState
              variant="inline"
              title="Create your first scene"
              subtitle="Build, preview, and publish your first Decentraland scene."
              actions={[{ label: "Start building", onClick: onStartBuilding }]}
            />
          </div>
          <PublishedScenesFooter />
        </>
      ) : (
        <>
          <div className="chh__cardbody">
            <div className="chh__list">
              {scenes.slice(0, 6).map((s) => (
                <CardItem key={s.id} title={s.title} icon={<LayersIcon />} href={s.href ?? null} />
              ))}
            </div>
          </div>
          <div className="chh__cardactions">
            <a className="chh__seeall" href="/create/scenes" onClick={onScenes} aria-label="See all scenes">See All</a>
          </div>
        </>
      )}
    </article>
  );
}

function ExploreCard() {
  return (
    <article className="chh__card">
      <CardBanner art={<CompassArt />} title="Explore" />
      <div className="chh__cardbody">
        <div className="chh__list">
          {EXPLORE_LINKS.map((l) => (
            <CardItem key={l.href} title={l.title} icon={l.icon} href={l.href} />
          ))}
        </div>
      </div>
    </article>
  );
}

type ManageCardProps = { committee?: boolean };

function ManageCard({ committee = false }: ManageCardProps) {
  const links = committee ? MANAGE_LINKS : MANAGE_LINKS.filter((l) => l.href !== "/create/curate");
  return (
    <article className="chh__card">
      <CardBanner art={<ManageArt />} title="Manage" />
      <div className="chh__cardbody">
        <div className="chh__list">
          {links.map((l) => (
            <CardItem key={l.href} title={l.title} icon={l.icon} href={l.href} builder={l.builder} badge={l.badge} />
          ))}
        </div>
      </div>
    </article>
  );
}

type LearnCardProps = { onLearn?: () => void };

function LearnCard({ onLearn }: LearnCardProps) {
  return (
    <article className="chh__card">
      <CardBanner art={<BookArt />} title="Learn" />
      <div className="chh__cardbody">
        <div className="chh__list">
          {LEARN_RESOURCES.map((r, i) => (
            <CardItem
              key={i}
              title={r.title}
              href={r.href}
              external
              icon={r.kind === "video" ? <VideoIcon /> : <BookmarkIcon />}
            />
          ))}
        </div>
      </div>
      <div className="chh__cardactions">
        <a className="chh__seeall" href="/create/learn" onClick={onLearn} aria-label="See all learning resources">See All</a>
      </div>
    </article>
  );
}

export type ChNetworkScene = {
  id: string;
  title: string;
  image: string | null;
  users: number;
  href: string;
};

export type ChTemplateCard = {
  id: string;
  title: string;
  thumb?: string;
  difficulty?: string;
  href: string;
};

export type ChHappening = {
  id: string;
  kind: "event" | "post";
  title: string;
  image?: string | null;
  hue?: number;
  meta: string;
  href: string;
  live?: boolean;
};

type StripHeaderProps = {
  title: string;
  linkLabel: string;
  href: string;
  onLink?: () => void;
};

function StripHeader({ title, linkLabel, href, onLink }: StripHeaderProps) {
  return (
    <div className="chh__stripheader">
      <h2 className="chh__striptitle">{title}</h2>
      <a className="chh__striplink" href={href} onClick={onLink}>
        {linkLabel}
      </a>
    </div>
  );
}

function LiveStrip({ scenes, onCardClick }: { scenes: ChNetworkScene[]; onCardClick?: (card: string) => void }) {
  return (
    <section className="chh__strip" aria-label="Live from the network">
      <StripHeader
        title="Live from the network"
        linkLabel="See all places"
        href="/places"
        onLink={() => onCardClick?.("network_all")}
      />
      <div className="chh__striprow">
        {scenes.map((s) => (
          <a key={s.id} className="chh__scenecard" href={s.href} onClick={() => onCardClick?.(`network:${s.id}`)}>
            <span className="chh__scenecardmedia">
              {s.image ? (
                <img className="chh__scenecardimg" src={s.image} alt="" loading="lazy" />
              ) : (
                <span className="chh__scenecardimg chh__scenecardimg--empty" aria-hidden="true" />
              )}
              <span className="chh__livechip">
                <span className="chh__livedot" aria-hidden="true" />
                {s.users} online
              </span>
            </span>
            <span className="chh__scenecardtitle u-truncate">{s.title}</span>
          </a>
        ))}
      </div>
    </section>
  );
}

function TemplatesStrip({ templates, onCardClick }: { templates: ChTemplateCard[]; onCardClick?: (card: string) => void }) {
  return (
    <section className="chh__strip" aria-label="Start from a template">
      <StripHeader
        title="Start from a template"
        linkLabel="Browse all templates"
        href="/create/templates"
        onLink={() => onCardClick?.("templates_all")}
      />
      <div className="chh__striprow">
        {templates.map((t) => (
          <a key={t.id} className="chh__templatecard" href={t.href} onClick={() => onCardClick?.(`template:${t.id}`)}>
            <span className="chh__scenecardmedia">
              {t.thumb ? (
                <img className="chh__scenecardimg" src={t.thumb} alt="" loading="lazy" />
              ) : (
                <span className="chh__scenecardimg chh__scenecardimg--empty" aria-hidden="true" />
              )}
              {t.difficulty ? <span className="chh__diffchip">{t.difficulty}</span> : null}
            </span>
            <span className="chh__scenecardtitle u-truncate">{t.title}</span>
          </a>
        ))}
      </div>
    </section>
  );
}

function HappeningsStrip({ happenings, onCardClick }: { happenings: ChHappening[]; onCardClick?: (card: string) => void }) {
  return (
    <section className="chh__strip" aria-label="What's happening">
      <StripHeader
        title="What's happening"
        linkLabel="See what's on"
        href="/whats-on"
        onLink={() => onCardClick?.("happenings_all")}
      />
      <div className="chh__striprow">
        {happenings.map((h) => (
          <a
            key={`${h.kind}:${h.id}`}
            className="chh__happeningcard"
            href={h.href}
            onClick={() => onCardClick?.(`${h.kind}:${h.id}`)}
          >
            <span className="chh__scenecardmedia">
              {h.image ? (
                <img className="chh__scenecardimg" src={h.image} alt="" loading="lazy" />
              ) : (
                <span
                  className="chh__scenecardimg chh__scenecardimg--hue"
                  style={h.hue != null ? { background: `linear-gradient(160deg, hsl(${h.hue} 55% 42%), hsl(${h.hue} 65% 22%))` } : undefined}
                  aria-hidden="true"
                />
              )}
              {h.live ? (
                <span className="chh__livechip">
                  <span className="chh__livedot" aria-hidden="true" />
                  LIVE
                </span>
              ) : null}
            </span>
            <span className="chh__happeningmeta">{h.meta}</span>
            <span className="chh__scenecardtitle chh__scenecardtitle--wrap">{h.title}</span>
          </a>
        ))}
      </div>
    </section>
  );
}

type CreatorHubHomeProps = {
  scenes?: ChScene[];
  scenesError?: boolean;
  rescoping?: boolean;
  signedIn?: boolean;
  account?: string;
  name?: string;
  committee?: boolean;
  chrome?: boolean;
  banner?: ReactNode;
  network?: ChNetworkScene[];
  templates?: ChTemplateCard[];
  happenings?: ChHappening[];
  onSignIn?: () => void;
  onStartBuilding?: () => void;
  onScenes?: () => void;
  onLearn?: () => void;
  onRetry?: () => void;
  onCardClick?: (card: string) => void;
};

export default function CreatorHubHome({
  scenes = [],
  scenesError = false,
  rescoping = false,
  signedIn = false,
  account = "",
  name = "",
  committee,
  chrome = true,
  banner = null,
  network = [],
  templates = [],
  happenings = [],
  onSignIn,
  onStartBuilding,
  onScenes,
  onLearn,
  onRetry,
  onCardClick,
}: CreatorHubHomeProps) {
  const auth = useChromeAuth();
  const isCommittee = committee ?? auth.committee;
  const hasScenes = scenes.length > 0;
  const greeting =
    signedIn && (name || account)
      ? `Welcome back, ${name || shortAddress(account)}`
      : "Welcome to Creator Hub";
  return (
    <CreatorHubChromeMaybe chrome={chrome} active="home" signedIn={signedIn} account={account} name={name} committee={isCommittee} onSignIn={onSignIn}>
      <section className="chh">
        <div className="chh__container">
          {banner}
          <div className="chh__hero">
            <div>
              <h1 className="chh__title">{greeting}</h1>
              {!signedIn ? (
                <p className="chh__herosub">
                  Build scenes, wearables and Worlds &#x2014; and see what the rest of Decentraland is making right now.
                </p>
              ) : null}
            </div>
            {!signedIn ? (
              <div className="chh__ctarow">
                <button type="button" className="chh__cta" onClick={onStartBuilding}>
                  Start building
                </button>
                <button type="button" className="chh__ghostbtn" onClick={onSignIn}>
                  Sign in
                </button>
              </div>
            ) : null}
          </div>

          <div className={hasScenes ? "chh__grid" : "chh__grid chh__grid--duo"}>
            <ScenesCard
              scenes={scenes}
              scenesError={scenesError}
              rescoping={rescoping}
              signedIn={signedIn}
              onSignIn={onSignIn}
              onScenes={onScenes}
              onStartBuilding={onStartBuilding}
              onRetry={onRetry}
            />
            {hasScenes ? <ExploreCard /> : null}
            {hasScenes ? <ManageCard committee={isCommittee} /> : null}
            <LearnCard onLearn={onLearn} />
          </div>

          {templates.length > 0 ? <TemplatesStrip templates={templates} onCardClick={onCardClick} /> : null}
          {happenings.length > 0 ? <HappeningsStrip happenings={happenings} onCardClick={onCardClick} /> : null}
          {/* Creation surfaces come first; the explore strip closes the page. */}
          {network.length > 0 ? <LiveStrip scenes={network} onCardClick={onCardClick} /> : null}
        </div>
      </section>
    </CreatorHubChromeMaybe>
  );
}
