import { siteBase } from "../../data/site";
import SitesChrome from "../frames/SitesChrome";
import "./stcastnotfound.css";

const SportsEsportsIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M21.58 16.09l-1.09-7.66A3.996 3.996 0 0 0 16.53 5H7.47C5.48 5 3.79 6.46 3.51 8.43l-1.09 7.66C2.2 17.63 3.39 19 4.94 19c.68 0 1.32-.27 1.8-.75L9 16h6l2.25 2.25c.48.48 1.13.75 1.8.75 1.56 0 2.75-1.37 2.53-2.91zM11 11H9v2H8v-2H6v-1h2V8h1v2h2v1zm4-1c-.55 0-1-.45-1-1s.45-1 1-1 1 .45 1 1-.45 1-1 1zm2 3c-.55 0-1-.45-1-1s.45-1 1-1 1 .45 1 1-.45 1-1 1z"
      fill="currentColor"
    />
  </svg>
);

const HomeIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path d="M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8z" fill="currentColor" />
  </svg>
);

const MenuBookIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M21 5c-1.11-.35-2.33-.5-3.5-.5-1.95 0-4.05.4-5.5 1.5-1.45-1.1-3.55-1.5-5.5-1.5S2.45 4.9 1 6v14.65c0 .25.25.5.5.5.1 0 .15-.05.25-.05C3.1 20.45 5.05 20 6.5 20c1.95 0 4.05.4 5.5 1.5 1.35-.85 3.8-1.5 5.5-1.5 1.65 0 3.35.3 4.75 1.05.1.05.15.05.25.05.25 0 .5-.25.5-.5V6c-.6-.45-1.25-.75-2-1zm0 13.5c-1.1-.35-2.3-.5-3.5-.5-1.7 0-4.15.65-5.5 1.5V8c1.35-.85 3.8-1.5 5.5-1.5 1.2 0 2.4.15 3.5.5v11.5z"
      fill="currentColor"
    />
  </svg>
);

type StCastNotFoundProps = {
  title?: string;
  description?: string;
  goHomeLabel?: string;
  viewDocsLabel?: string;
  homeHref?: string;
  docsHref?: string;
};

export default function StCastNotFound({
  title = "404 - Page Not Found",
  description = "This Cast 2.0 stream doesn't exist. Stream links are generated on-demand by the Admin Smart Item in Decentraland scenes. If you followed a valid link and still see this error, the stream may have ended.",
  goHomeLabel = "Go Home",
  viewDocsLabel = "View Documentation",
  homeHref = siteBase(),
  docsHref = "https://docs.decentraland.org/creator/worlds/cast/",
}: StCastNotFoundProps) {
  return (
    <SitesChrome active="legal" overlayNav>
      <div className="stcastnotfound">
        <div className="stcastnotfound__icon" aria-hidden="true">
          <SportsEsportsIcon />
        </div>

        <h1 className="stcastnotfound__title">{title}</h1>
        <p className="stcastnotfound__desc">{description}</p>

        <div className="stcastnotfound__buttons">
          <a className="stcastnotfound__btn" href={homeHref}>
            <span className="stcastnotfound__btnicon" aria-hidden="true">
              <HomeIcon />
            </span>
            {goHomeLabel}
          </a>
          <a
            className="stcastnotfound__link"
            href={docsHref}
            target="_blank"
            rel="noopener noreferrer"
          >
            <MenuBookIcon />
            {viewDocsLabel}
          </a>
        </div>
      </div>
    </SitesChrome>
  );
}
