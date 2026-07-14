import SitesChrome from "../frames/SitesChrome";
import { asset } from "../../asset";
import "./stcreatorhubdownload.css";

const L = {
  title: "Decentraland Creator Hub",
  alsoAvailable: "Also available on",
  downloadFor: (os: string) => `Download for ${os}`,
};

export const ICON = {
  Windows:
    "data:image/svg+xml;utf8," +
    encodeURIComponent(
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#fff" d="M0 3.449 9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699m10.949-8.099H24V24l-12.9-1.801"/></svg>',
    ),
  macOS:
    "data:image/svg+xml;utf8," +
    encodeURIComponent(
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#fff" d="M12.152 6.896c-.948 0-2.415-1.078-3.96-1.04-2.04.027-3.91 1.183-4.961 3.014-2.117 3.675-.546 9.103 1.519 12.09 1.013 1.454 2.208 3.09 3.792 3.039 1.52-.065 2.09-.987 3.935-.987 1.831 0 2.35.987 3.96.948 1.637-.026 2.676-1.48 3.676-2.948 1.156-1.688 1.636-3.325 1.662-3.415-.039-.013-3.182-1.221-3.22-4.857-.026-3.04 2.48-4.494 2.597-4.559-1.429-2.09-3.623-2.324-4.39-2.376-2-.156-3.675 1.09-4.61 1.09zM15.53 3.83c.843-1.012 1.4-2.427 1.245-3.83-1.207.052-2.662.805-3.532 1.818-.78.896-1.454 2.338-1.273 3.714 1.338.104 2.715-.688 3.559-1.701"/></svg>',
    ),
};

type DownloadOption = {
  text: string;
  image: string;
  link: string;
  arch: string;
};

const PRIMARY: DownloadOption = {
  text: "macOS",
  image: ICON.macOS,
  link: "https://github.com/decentraland/creator-hub/releases/latest",
  arch: "arm64",
};
const SECONDARY: DownloadOption[] = [
  {
    text: "Windows",
    image: ICON.Windows,
    link: "https://github.com/decentraland/creator-hub/releases/latest",
    arch: "amd64",
  },
];

function DclLogo() {
  return (
    <div className="stcreatorhubdownload__logo" aria-label="Decentraland">
      <img src={asset("assets/dcl-logo.png")} alt="" width="40" height="40" />
      <span className="stcreatorhubdownload__wordmark">decentraland</span>
    </div>
  );
}

function BannerArt() {
  return (
    <img
      className="stcreatorhubdownload__banner"
      src={asset("assets/creator-hub-banner.png")}
      alt="Decentraland Creator Hub"
    />
  );
}

const DownloadGlyph = ({ src, alt }: { src: string; alt: string }) => (
  <img className="stcreatorhubdownload__btnicon" src={src} alt={alt} width="20" height="20" />
);

type StCreatorHubDownloadProps = {
  primaryOption?: DownloadOption;
  secondaryOptions?: DownloadOption[];
};

export default function StCreatorHubDownload({
  primaryOption = PRIMARY,
  secondaryOptions = SECONDARY,
}: StCreatorHubDownloadProps) {
  return (
    <SitesChrome active="create" overlayNav>
      <main className="stcreatorhubdownload">
        <div className="stcreatorhubdownload__card">
          <div className="stcreatorhubdownload__info">
            <DclLogo />
            <h1 className="stcreatorhubdownload__title">{L.title}</h1>

            {primaryOption?.link && (
              <div className="stcreatorhubdownload__actions">
                <a
                  className="stcreatorhubdownload__cta"
                  href={primaryOption.link}
                  data-os={primaryOption.text}
                >
                  <span className="stcreatorhubdownload__cta-label">
                    {L.downloadFor(primaryOption.text)}
                  </span>
                  <DownloadGlyph src={primaryOption.image} alt="" />
                </a>

                {secondaryOptions.length > 0 && (
                  <div className="stcreatorhubdownload__also">
                    <span className="stcreatorhubdownload__also-text">{L.alsoAvailable}</span>
                    {secondaryOptions.map((option) => (
                      <a
                        key={option.text}
                        className="stcreatorhubdownload__alticon"
                        href={option.link}
                        data-os={option.text}
                        aria-label={L.downloadFor(option.text)}
                      >
                        <img src={option.image} alt="" width="20" height="20" />
                      </a>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>

          <BannerArt />
        </div>
      </main>
    </SitesChrome>
  );
}
