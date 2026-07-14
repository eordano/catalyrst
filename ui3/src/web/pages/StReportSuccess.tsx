import SitesChrome from "../frames/SitesChrome";
import "./streportsuccess.css";

const L = {
  title: "Report Submitted",
  body: "Your report has been received and will be reviewed shortly.",
  dismiss: "You can close this window now.",
  scenesTitle: "Jump into these scenes",
};

const SCENES = [
  { name: "Genesis Plaza", location: "0,0", href: "https://decentraland.org/play/?position=0,0" },
  { name: "Wonder Mile", location: "-9,-9", href: "https://decentraland.org/play/?position=-9,-9" },
  { name: "Casino District", location: "137,87", href: "https://decentraland.org/play/?position=137,87" },
];

const JumpPin = () => (
  <svg className="streportsuccess__scenepin" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
    <path
      fill="currentColor"
      d="M12 2a7 7 0 0 0-7 7c0 5.25 7 13 7 13s7-7.75 7-13a7 7 0 0 0-7-7Zm0 9.5A2.5 2.5 0 1 1 12 6.5a2.5 2.5 0 0 1 0 5Z"
    />
  </svg>
);

const SuccessLogo = () => (
  <svg
    className="streportsuccess__logo"
    width="64"
    height="64"
    viewBox="0 0 90 90"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M45 89.9999C69.8528 89.9999 89.9999 69.8528 89.9999 45C89.9999 20.1472 69.8528 0 45 0C20.1472 0 0 20.1472 0 45C0 69.8528 20.1472 89.9999 45 89.9999Z"
      fill="url(#streportsuccess_p0)"
    />
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M18 80.9996C25.515 86.6471 34.875 89.9996 45 89.9996C55.125 89.9996 64.485 86.6471 72 80.9996H18Z"
      fill="#FF2D55"
    />
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M9.00008 71.9996C11.5651 75.3971 14.6026 78.4346 18.0001 80.9996H72C75.3975 78.4346 78.435 75.3971 81 71.9996H9.00008Z"
      fill="#FFA25A"
    />
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M60.3675 62.9997H3.75757C5.15257 66.2172 6.93007 69.2322 9.00006 71.9997H60.39V62.9997H60.3675Z"
      fill="#FFC95B"
    />
    <path fillRule="evenodd" clipRule="evenodd" d="M31.8826 29.2496V62.9996H60.0075L31.8826 29.2496Z" fill="url(#streportsuccess_p1)" />
    <path fillRule="evenodd" clipRule="evenodd" d="M3.75757 62.9997H31.8826V29.2497L3.75757 62.9997Z" fill="white" />
    <path fillRule="evenodd" clipRule="evenodd" d="M60.3675 47.25V72H81L60.3675 47.25Z" fill="url(#streportsuccess_p2)" />
    <path fillRule="evenodd" clipRule="evenodd" d="M39.7574 72H60.3674V47.25L39.7574 72Z" fill="white" />
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M60.3675 40.4997C66.5807 40.4997 71.6175 35.4629 71.6175 29.2497C71.6175 23.0365 66.5807 17.9997 60.3675 17.9997C54.1543 17.9997 49.1175 23.0365 49.1175 29.2497C49.1175 35.4629 54.1543 40.4997 60.3675 40.4997Z"
      fill="#FFC95B"
    />
    <path
      fillRule="evenodd"
      clipRule="evenodd"
      d="M31.8824 22.4997C34.989 22.4997 37.5074 19.9813 37.5074 16.8747C37.5074 13.7681 34.989 11.2497 31.8824 11.2497C28.7758 11.2497 26.2574 13.7681 26.2574 16.8747C26.2574 19.9813 28.7758 22.4997 31.8824 22.4997Z"
      fill="#FFC95B"
    />
    <defs>
      <linearGradient id="streportsuccess_p0" x1="45" y1="-18.6396" x2="-18.6396" y2="45" gradientUnits="userSpaceOnUse">
        <stop stopColor="#FF2D55" />
        <stop offset="1" stopColor="#FFBC5B" />
      </linearGradient>
      <linearGradient id="streportsuccess_p1" x1="31.8731" y1="29.2497" x2="31.8731" y2="62.9996" gradientUnits="userSpaceOnUse">
        <stop stopColor="#A524B3" />
        <stop offset="1" stopColor="#FF2D55" />
      </linearGradient>
      <linearGradient id="streportsuccess_p2" x1="60.3605" y1="47.25" x2="60.3605" y2="72" gradientUnits="userSpaceOnUse">
        <stop stopColor="#A524B3" />
        <stop offset="1" stopColor="#FF2D55" />
      </linearGradient>
    </defs>
  </svg>
);

export default function StReportSuccess() {
  return (
    <SitesChrome>
      <main className="streportsuccess">
        <div className="streportsuccess__bg" aria-hidden="true" />

        <div className="streportsuccess__content">
          <div className="streportsuccess__card">
            <div className="streportsuccess__logowrap">
              <SuccessLogo />
              <h1 className="streportsuccess__title">{L.title}</h1>
            </div>

            <div className="streportsuccess__textgroup">
              <p className="streportsuccess__body">{L.body}</p>
              <p className="streportsuccess__secondary">{L.dismiss}</p>
            </div>

            <div className="streportsuccess__scenes">
              <h2 className="streportsuccess__scenestitle">{L.scenesTitle}</h2>
              <div className="streportsuccess__scenelist">
                {SCENES.map((scene) => (
                  <a
                    key={scene.name}
                    className="streportsuccess__scene"
                    href={scene.href}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    <span className="streportsuccess__scenethumb" aria-hidden="true" />
                    <span className="streportsuccess__scenemeta">
                      <span className="streportsuccess__scenename">{scene.name}</span>
                      <span className="streportsuccess__sceneloc">
                        <JumpPin />
                        {scene.location}
                      </span>
                    </span>
                  </a>
                ))}
              </div>
            </div>
          </div>
        </div>
      </main>
    </SitesChrome>
  );
}
