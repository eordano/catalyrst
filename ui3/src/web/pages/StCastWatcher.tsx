import type { ReactNode } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import "./stcastwatcher.css";

const TvIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M21 3H3c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h5v2h8v-2h5c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2Zm0 14H3V5h18v12Z"
      fill="currentColor"
    />
  </svg>
);

const VolumeUpIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M3 9v6h4l5 5V4L7 9H3Zm13.5 3A4.5 4.5 0 0 0 14 7.97v8.05A4.5 4.5 0 0 0 16.5 12ZM14 3.23v2.06a7 7 0 0 1 0 13.42v2.06a9 9 0 0 0 0-17.54Z"
      fill="currentColor"
    />
  </svg>
);

const VolumeOffIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M16.5 12A4.5 4.5 0 0 0 14 7.97v2.18l2.45 2.45c.03-.2.05-.4.05-.6ZM19 12c0 .94-.2 1.82-.54 2.64l1.51 1.51A8.8 8.8 0 0 0 21 12a9 9 0 0 0-7-8.77v2.06A7 7 0 0 1 19 12ZM4.27 3 3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25A7 7 0 0 1 14 18.7v2.06a9 9 0 0 0 3.69-1.81L19.73 21 21 19.73 4.27 3ZM12 4 9.91 6.09 12 8.18V4Z"
      fill="currentColor"
    />
  </svg>
);

const ChatIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2Zm0 14H5.17L4 17.17V4h16v12Z"
      fill="currentColor"
    />
  </svg>
);

const PeopleIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M16 11c1.66 0 3-1.34 3-3s-1.34-3-3-3-3 1.34-3 3 1.34 3 3 3Zm-8 0c1.66 0 3-1.34 3-3S9.66 5 8 5 5 6.34 5 8s1.34 3 3 3Zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5Zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5Z"
      fill="currentColor"
    />
  </svg>
);

const CallEndIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M12 9c-1.6 0-3.15.25-4.6.72v3.1c0 .39-.23.74-.56.9-.98.49-1.87 1.12-2.66 1.85-.18.18-.43.28-.7.28-.28 0-.53-.11-.71-.29L.29 13.08a.996.996 0 0 1-.29-.7c0-.28.11-.53.29-.71C3.34 8.78 7.46 7 12 7s8.66 1.78 11.71 4.67c.18.18.29.43.29.71 0 .28-.11.53-.29.71l-2.48 2.48c-.18.18-.43.29-.71.29-.27 0-.52-.11-.7-.28a11.27 11.27 0 0 0-2.66-1.85.998.998 0 0 1-.56-.9v-3.1C15.15 9.25 13.6 9 12 9Z"
      fill="currentColor"
    />
  </svg>
);

const ChevronDownIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path d="M7 10l5 5 5-5H7Z" fill="currentColor" />
  </svg>
);

const CloseIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12 19 6.41Z"
      fill="currentColor"
    />
  </svg>
);

const DclLogo = () => (
  <svg viewBox="0 0 48 48" aria-hidden="true">
    <circle cx="24" cy="24" r="24" fill="#ff2d55" />
    <path d="M24 11 13 28h22L24 11Zm-13 19v3a4 4 0 0 0 4 4h18a4 4 0 0 0 4-4v-3H11Z" fill="#fff" />
    <circle cx="18" cy="33" r="2.4" fill="#fff" />
    <circle cx="30" cy="33" r="2.4" fill="#fff" />
  </svg>
);

function LivePill({ label }: { label: ReactNode }) {
  return (
    <div className="stcastwatcher__counter">
      <div className="stcastwatcher__livepill">
        <span className="stcastwatcher__livedot" aria-hidden="true" />
        {label}
      </div>
    </div>
  );
}

function WatcherEmptyState({ title, message }: { title: ReactNode; message: ReactNode }) {
  return (
    <div className="stcastwatcher__empty">
      <div className="stcastwatcher__emptyicon" aria-hidden="true">
        <TvIcon />
      </div>
      <h2 className="stcastwatcher__emptytitle">{title}</h2>
      <p className="stcastwatcher__emptysub">{message}</p>
    </div>
  );
}

type ChatMessage = { name: string; time: string; body: string };

function ChatSidebar({
  title,
  messages,
  footer,
  onClose,
}: {
  title: ReactNode;
  messages: ChatMessage[];
  footer: ReactNode;
  onClose: () => void;
}) {
  return (
    <>
      <div className="stcastwatcher__sidehead">
        <h2 className="stcastwatcher__sidetitle">{title}</h2>
        <button type="button" className="stcastwatcher__sideclose" aria-label="Close" onClick={onClose}>
          <CloseIcon />
        </button>
      </div>
      <div className="stcastwatcher__chatmsgs">
        {messages.map((m, i) => (
          <div key={i} className="stcastwatcher__msg">
            <div className="stcastwatcher__msgmeta">
              <span className="stcastwatcher__msgname">{m.name}</span>
              <span className="stcastwatcher__msgtime">{m.time}</span>
            </div>
            <div className="stcastwatcher__msgbody">{m.body}</div>
          </div>
        ))}
      </div>
      <div className="stcastwatcher__chatfoot">{footer}</div>
    </>
  );
}

const DEMO_MESSAGES: ChatMessage[] = [
  { name: "0x7a3f...c21d", time: "2:14 PM", body: "this build looks incredible \u{1F525}" },
  { name: "metahopper.dcl.eth", time: "2:15 PM", body: "where can i grab those wearables?" },
  { name: "0x91be...4f08", time: "2:16 PM", body: "gm everyone, just jumped in" },
];

type WatcherState = "onboarding" | "joining" | "live" | "waiting";
type WatcherToastAction = { label?: string; onClick?: () => void };
type WatcherToast = { title: ReactNode; message: ReactNode; action?: string | WatcherToastAction };

type StCastWatcherProps = {
  chrome?: boolean;
  streamName?: string;
  state?: WatcherState;
  sidebarOpen?: boolean;
  isTabMuted?: boolean;
  viewerTag?: ReactNode;
  joinLabel?: ReactNode;
  joiningLabel?: ReactNode;
  leaveLabel?: ReactNode;
  liveLabel?: ReactNode;
  chatTitle?: ReactNode;
  chatFooter?: ReactNode;
  watcherTitle?: ReactNode;
  watcherMessage?: ReactNode;
  toasts?: WatcherToast[];
  participantCount?: number;
  unreadCount?: number;
};

export default function StCastWatcher({
  chrome = true,
  streamName = "Genesis Plaza",
  state = "live",
  sidebarOpen = true,
  isTabMuted = false,
  viewerTag = "Viewer",
  joinLabel = "JOIN NOW",
  joiningLabel = "Joining...",
  leaveLabel = "Leave",
  liveLabel = "LIVE",
  chatTitle = "In-World Chat",
  chatFooter = "Jump into Genesis Plaza in Decentraland to participate in the chat.",
  watcherTitle = "No one is currently casting to this link...",
  watcherMessage = "This Decentraland Cast may be starting soon or has already ended.",
  toasts = [],
  participantCount = 12,
  unreadCount = 0,
}: StCastWatcherProps) {
  const onboardTitle = `Decentraland Cast from ${streamName}`;

  let body: ReactNode;
  if (state === "onboarding") {
    body = (
      <div className="stcastwatcher__onboard">
        <div className="stcastwatcher__modal">
          <span className="stcastwatcher__logo">
            <DclLogo />
          </span>
          <h1 className="stcastwatcher__title">{onboardTitle}</h1>
          <p className="stcastwatcher__tag">{viewerTag}</p>

          <div className="stcastwatcher__selectors">
            <div className="stcastwatcher__selectorrow">
              <VolumeUpIcon />
              <button type="button" className="stcastwatcher__selectorbtn">
                <span>Default - Speakers</span>
                <ChevronDownIcon />
              </button>
            </div>
          </div>

          <button type="button" className="stcastwatcher__join">
            {joinLabel}
          </button>
        </div>
      </div>
    );
  } else if (state === "joining") {
    body = (
      <div className="stcastwatcher__onboard">
        <div className="stcastwatcher__joining">
          <span className="stcastwatcher__joininglogo">
            <DclLogo />
          </span>
          <span className="stcastwatcher__spinner" aria-hidden="true" />
          <p className="stcastwatcher__joiningtext">{joiningLabel}</p>
        </div>
      </div>
    );
  } else {
    const showSidebar = state === "live" && sidebarOpen;
    body = (
      <div
        className={
          "stcastwatcher__layout" + (showSidebar ? " stcastwatcher--sidebar" : "")
        }
      >
        <div className="stcastwatcher__main">
          <div className="stcastwatcher__videocontainer">
            <div className="stcastwatcher__videoarea">
              <div className="stcastwatcher__content">
                {state === "waiting" ? (
                  <WatcherEmptyState title={watcherTitle} message={watcherMessage} />
                ) : (
                  <>
                    <LivePill label={liveLabel} />
                    <div
                      className="stcastwatcher__frame"
                      role="img"
                      aria-label={`Live cast from ${streamName}`}
                    >
                      <span className="stcastwatcher__framelogo" aria-hidden="true">
                        <DclLogo />
                      </span>
                    </div>
                  </>
                )}
              </div>
            </div>

            <aside
              className="stcastwatcher__sidebar"
              aria-hidden={!showSidebar}
              inert={!showSidebar}
            >
              <ChatSidebar
                title={chatTitle}
                messages={DEMO_MESSAGES}
                footer={chatFooter}
                onClose={() => {}}
              />
            </aside>
          </div>

          <div className="stcastwatcher__controlsarea">
            <div className="stcastwatcher__controls">
              <div className="stcastwatcher__controlscenter">
                <button type="button" className="stcastwatcher__leave">
                  <CallEndIcon />
                  {leaveLabel}
                </button>
              </div>

              <div className="stcastwatcher__controlsright">
                <button
                  type="button"
                  className={"stcastwatcher__iconbtn" + (isTabMuted ? " is-active" : "")}
                  title={isTabMuted ? "Unmute Cast audio" : "Mute this Cast audio while you're in World"}
                  aria-label={isTabMuted ? "Unmute Cast audio" : "Mute Cast audio"}
                >
                  {isTabMuted ? <VolumeOffIcon /> : <VolumeUpIcon />}
                </button>
                <button
                  type="button"
                  className={"stcastwatcher__iconbtn" + (showSidebar ? " is-active" : "")}
                  aria-label="Chat"
                  aria-expanded={showSidebar}
                >
                  <ChatIcon />
                  {unreadCount > 0 && <span className="stcastwatcher__badge">{unreadCount}</span>}
                </button>
                <button type="button" className="stcastwatcher__iconbtn" aria-label="Participants">
                  <PeopleIcon />
                  <span className="stcastwatcher__badge">{participantCount}</span>
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <SitesChromeMaybe chrome={chrome} active="legal">
      <div className="stcastwatcher">
        {body}

        {toasts.length > 0 && (
          <div className="stcastwatcher__toasts">
            {toasts.map((toast, i) => (
              <div key={i} className="stcastwatcher__toast" role="status">
                <div className="stcastwatcher__toastrow">
                  <div className="stcastwatcher__toasttext">
                    <div className="stcastwatcher__toasttitle">{toast.title}</div>
                    <div className="stcastwatcher__toastmsg">{toast.message}</div>
                  </div>
                  <button type="button" className="stcastwatcher__toastclose" aria-label="Dismiss">
                    <CloseIcon />
                  </button>
                </div>
                {toast.action && (
                  <button
                    type="button"
                    className="stcastwatcher__toastaction"
                    onClick={typeof toast.action === "string" ? undefined : toast.action.onClick}
                  >
                    {typeof toast.action === "string" ? toast.action : toast.action.label}
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </SitesChromeMaybe>
  );
}
