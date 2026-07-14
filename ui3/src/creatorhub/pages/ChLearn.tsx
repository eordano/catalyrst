import { CreatorHubChromeMaybe, type CreatorHubNavId } from "../frames/CreatorHubChrome";
import "./chlearn.css";

const VideosArt = () => (
  <svg className="chl__art" viewBox="0 0 150 100" aria-hidden="true">
    <rect x="22" y="30" width="74" height="48" rx="6" fill="var(--ink-85)" opacity="0.92" />
    <rect x="22" y="30" width="74" height="48" rx="6" fill="none" stroke="var(--line)" strokeWidth="2.5" />
    <path d="M52 44v20l18-10-18-10Z" fill="var(--brand)" />
    <circle cx="104" cy="34" r="12" fill="none" stroke="var(--line)" strokeWidth="2.5" />
    <path d="M99 34h10M104 29v10" stroke="var(--ink-85)" strokeWidth="2.4" strokeLinecap="round" />
  </svg>
);
const DocsArt = () => (
  <svg className="chl__art" viewBox="0 0 150 100" aria-hidden="true">
    <rect x="34" y="22" width="56" height="68" rx="5" fill="var(--ink-85)" opacity="0.92" />
    <rect x="34" y="22" width="56" height="68" rx="5" fill="none" stroke="var(--line)" strokeWidth="2.5" />
    <path d="M44 38h36M44 50h36M44 62h26" stroke="var(--brand)" strokeWidth="3" strokeLinecap="round" />
    <circle cx="98" cy="68" r="13" fill="none" stroke="var(--line)" strokeWidth="2.5" />
    <path d="M94 68h8M98 64v8" stroke="var(--ink-85)" strokeWidth="2.4" strokeLinecap="round" />
  </svg>
);
const MoreArt = () => (
  <svg className="chl__art" viewBox="0 0 150 100" aria-hidden="true">
    <circle cx="58" cy="50" r="22" fill="var(--ink-85)" opacity="0.92" />
    <circle cx="58" cy="50" r="22" fill="none" stroke="var(--line)" strokeWidth="2.5" />
    <circle cx="49" cy="50" r="3.4" fill="var(--brand)" />
    <circle cx="58" cy="50" r="3.4" fill="var(--brand)" />
    <circle cx="67" cy="50" r="3.4" fill="var(--brand)" />
    <path d="M92 40l14 10-14 10V40Z" fill="var(--ink-85)" />
  </svg>
);

const DocsIcon = () => (
  <i className="chl__icon" aria-hidden="true">
    <svg viewBox="0 0 24 24" width="24" height="24">
      <path d="M7 3h7l5 5v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
      <path d="M14 3v5h5" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
      <path d="M8.5 13h7M8.5 16.5h7" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  </i>
);
const LinkIcon = () => (
  <i className="chl__icon" aria-hidden="true">
    <svg viewBox="0 0 24 24" width="24" height="24">
      <path d="M14 4h6v6M20 4l-9 9" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M18 13v5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h5" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  </i>
);

type VideoLinkProps = { id: string; list: string; title: string };

function VideoLink({ id, list, title }: VideoLinkProps) {
  const url = `https://youtu.be/${id}?list=${list}`;
  return (
    <a className="chl__link" href={url} target="_blank" rel="noreferrer">
      <span className="chl__thumb">
        <img src={`https://img.youtube.com/vi/${id}/0.jpg`} alt="" />
      </span>
      <span className="chl__linktitle">{title}</span>
    </a>
  );
}

type ResourceLinkProps = { url: string; title: string; kind: string };

function ResourceLink({ url, title, kind }: ResourceLinkProps) {
  const external = url.startsWith("http");
  return (
    <a
      className="chl__link"
      href={url}
      {...(external ? { target: "_blank", rel: "noreferrer" } : {})}
    >
      {kind === "docs" ? <DocsIcon /> : <LinkIcon />}
      <span className="chl__linktitle">{title}</span>
    </a>
  );
}

const VIDEOS = [
  { list: "PLAcRraQmr_GMJw77zKvN84LX_OLyn-lVz", id: "nWiyoX70vtc", title: "Project Updates" },
  { list: "PLAcRraQmr_GP_K8WN7csnKnImK4R2TgMA", id: "52LiG-4VI9c", title: "Editor (no code)" },
  { list: "PLAcRraQmr_GN8LcnnQk2BByo9L2Orvp9c", id: "-iWslh4uQIk", title: "Emote Tutorials" },
  { list: "PLAcRraQmr_GP_K8WN7csnKnImK4R2TgMA", id: "J_EO1LZkaiA", title: "Combine drag & drop + Code" },
] as const;

type Resource = { url: string; title: string };

const DOCS: Resource[] = [
  { url: "https://docs.decentraland.org/creator/", title: "Let's build the metaverse together" },
  { url: "https://docs.decentraland.org/creator/scenes-sdk7/getting-started/sdk-101", title: "About SDK" },
  { url: "https://docs.decentraland.org/creator/scenes-sdk7/getting-started/dev-workflow", title: "Development Workflow" },
  { url: "https://docs.decentraland.org/creator/wearables/wearables-overview/", title: "Wearable Overview" },
  { url: "https://docs.decentraland.org/creator/emotes/emotes-overview/", title: "Emotes Overview" },
  { url: "https://docs.decentraland.org/creator/wearables-and-emotes/manage-collections/creating-a-collection", title: "Creating a Collection" },
];

const MORE: Resource[] = [
  { url: "https://docs.decentraland.org/contributor/", title: "Open Protocol Docs" },
  { url: "/creator-hub/map", title: "Creator Hub flow map" },
  { url: "/explorer-map", title: "Explorer flow map" },
];

const VIDEOS_PLAYLIST_URL = `https://www.youtube.com/playlist?list=${VIDEOS[0].list}`;
const DOCS_URL = "https://docs.decentraland.org/creator/";

type ChLearnProps = {
  active?: CreatorHubNavId;
  signedIn?: boolean;
  chrome?: boolean;
  account?: string;
  name?: string;
  onSignIn?: () => void;
};

export default function ChLearn({
  active = "learn",
  signedIn = false,
  account = "",
  name = "",
  onSignIn,
  chrome = true,
}: ChLearnProps) {
  return (
    <CreatorHubChromeMaybe chrome={chrome} active={active} signedIn={signedIn} account={account} name={name} onSignIn={onSignIn}>
      <section className="chl">
        <div className="chl__container">
          <div className="chl__content">
            <h1 className="chl__title">Learn</h1>

            <div className="chl__sections">
              <section className="chl__section chl__section--videos">
                <a
                  className="chl__header chl__header--clickable"
                  href={VIDEOS_PLAYLIST_URL}
                  target="_blank"
                  rel="noreferrer"
                >
                  <span className="chl__image"><VideosArt /></span>
                  <h2 className="chl__sectiontitle">Videos</h2>
                </a>
                <div className="chl__sectioncontent">
                  {VIDEOS.map((v, i) => (
                    <VideoLink key={i} {...v} />
                  ))}
                </div>
                <a
                  className="chl__seeall"
                  href={VIDEOS_PLAYLIST_URL}
                  target="_blank"
                  rel="noreferrer"
                >
                  See All
                </a>
              </section>

              <section className="chl__section chl__section--docs">
                <a
                  className="chl__header chl__header--clickable"
                  href={DOCS_URL}
                  target="_blank"
                  rel="noreferrer"
                >
                  <span className="chl__image"><DocsArt /></span>
                  <h2 className="chl__sectiontitle">Creator Docs</h2>
                </a>
                <div className="chl__sectioncontent">
                  {DOCS.map((d, i) => (
                    <ResourceLink key={i} {...d} kind="docs" />
                  ))}
                </div>
                <a
                  className="chl__seeall"
                  href={DOCS_URL}
                  target="_blank"
                  rel="noreferrer"
                >
                  See All
                </a>
              </section>

              <section className="chl__section chl__section--more">
                <div className="chl__header">
                  <span className="chl__image"><MoreArt /></span>
                  <h2 className="chl__sectiontitle">More</h2>
                </div>
                <div className="chl__sectioncontent">
                  {MORE.map((m, i) => (
                    <ResourceLink key={i} {...m} kind="more" />
                  ))}
                </div>
              </section>
            </div>
          </div>
        </div>
      </section>
    </CreatorHubChromeMaybe>
  );
}
