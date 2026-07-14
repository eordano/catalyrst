import { CreatorHubChromeMaybe } from "../frames/CreatorHubChrome";
import EmptyStateCard from "../../components/EmptyStateCard";
import "./chscenesemptystate.css";

const ImportIcon = () => (
  <svg viewBox="0 0 20 16" width="20" height="16" aria-hidden="true">
    <path d="M10 1v9M6.5 7l3.5 3 3.5-3" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    <path d="M3 12.5v1.2A1.3 1.3 0 0 0 4.3 15h11.4a1.3 1.3 0 0 0 1.3-1.3v-1.2" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" />
  </svg>
);
const TemplateIcon = () => (
  <svg viewBox="0 0 20 16" width="20" height="16" aria-hidden="true">
    <rect x="2.5" y="2" width="6" height="5" rx="1" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <rect x="11.5" y="2" width="6" height="5" rx="1" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <rect x="2.5" y="9" width="6" height="5" rx="1" stroke="currentColor" strokeWidth="1.5" fill="none" />
    <rect x="11.5" y="9" width="6" height="5" rx="1" stroke="currentColor" strokeWidth="1.5" fill="none" />
  </svg>
);

type EmptyStateButtonProps = { onCreateScene?: () => void };

function EmptyStateButton({ onCreateScene }: EmptyStateButtonProps) {
  return (
    <button
      type="button"
      className="chscenesemptystate__nobutton"
      aria-label="Create your first scene"
      onClick={() => onCreateScene?.()}
    >
      <svg
        className="chscenesemptystate__nobtnsvg"
        width="256"
        height="234"
        viewBox="0 0 256 234"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden="true"
      >
        <defs>
          <linearGradient id="chesGrad" x1="128" y1="0" x2="128" y2="234" gradientUnits="userSpaceOnUse">
            <stop stopColor="var(--brand)" />
            <stop offset="1" stopColor="var(--brand)" />
          </linearGradient>
          <linearGradient id="chesFill" x1="128" y1="0" x2="128" y2="234" gradientUnits="userSpaceOnUse">
            <stop stopColor="var(--purple)" />
            <stop offset="1" stopColor="var(--purple)" />
          </linearGradient>
        </defs>
        <rect
          className="chscenesemptystate__nobtnfill"
          x="1.5"
          y="1.5"
          width="253"
          height="231"
          rx="8.5"
          fill="url(#chesFill)"
          fillOpacity="0.25"
        />
        <rect
          x="1.5"
          y="1.5"
          width="253"
          height="231"
          rx="8.5"
          stroke="url(#chesGrad)"
          strokeWidth="3"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeDasharray="16 16"
        />
        <path
          d="M168 79.7143H133.714V114H122.286V79.7143H88V68.2857H122.286V34H133.714V68.2857H168V79.7143Z"
          fill="url(#chesGrad)"
        />
        <text className="chscenesemptystate__nobtnlabel" x="128" y="188" textAnchor="middle" fill="url(#chesGrad)">
          Start building
        </text>
      </svg>
    </button>
  );
}

type ChScenesEmptyStateProps = {
  signedIn?: boolean;
  chrome?: boolean;
  account?: string;
  name?: string;
  onSignIn?: () => void;
  onCreateScene?: () => void;
  onImport?: () => void;
  onTemplates?: () => void;
};

export default function ChScenesEmptyState({
  signedIn = false,
  account = "",
  name = "",
  onSignIn,
  onCreateScene,
  onImport,
  onTemplates,
  chrome = true,
}: ChScenesEmptyStateProps) {
  return (
    <CreatorHubChromeMaybe chrome={chrome} active="scenes" signedIn={signedIn} account={account} name={name} onSignIn={onSignIn}>
      <section className="chscenesemptystate">
        <div className="chscenesemptystate__container">
          <div className="chscenesemptystate__list-wrap">
            <div className="chscenesemptystate__menu">
              <div className="chscenesemptystate__header">
                <h1 className="chscenesemptystate__heading">My Scenes</h1>
                <div className="chscenesemptystate__actions">
                  <button
                    type="button"
                    className="chscenesemptystate__actionbtn chscenesemptystate__actionbtn--secondary"
                    onClick={() => onImport?.()}
                  >
                    <ImportIcon />
                    Import Scene
                  </button>
                  <button
                    type="button"
                    className="chscenesemptystate__actionbtn chscenesemptystate__actionbtn--secondary"
                    onClick={() => onTemplates?.()}
                  >
                    <TemplateIcon />
                    Templates
                  </button>
                </div>
              </div>
            </div>

            <div className="chscenesemptystate__nocontainer">
              {signedIn ? (
                <EmptyStateCard
                  className="chscenesemptystate__card"
                  title="Create your first scene"
                  subtitle={
                    <>
                      Unleash your creativity. Start building scenes for your LANDs and
                      Worlds and share with the community.{" "}
                      <a
                        href="https://docs.decentraland.org/creator/scenes-sdk7/getting-started/sdk-101"
                        target="_blank"
                        rel="noreferrer"
                      >
                        Learn more about creating Scenes.
                      </a>
                    </>
                  }
                  actionsGap={48}
                  actions={<EmptyStateButton onCreateScene={onCreateScene} />}
                />
              ) : (
                <EmptyStateCard
                  className="chscenesemptystate__card"
                  title="Sign in to see your scenes"
                  subtitle="Your scenes will appear here once your wallet is connected."
                  actions={[{ label: "Sign in", onClick: onSignIn }]}
                />
              )}
            </div>
          </div>
        </div>
      </section>
    </CreatorHubChromeMaybe>
  );
}
