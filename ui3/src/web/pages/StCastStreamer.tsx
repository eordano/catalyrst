import type { CSSProperties, ReactNode } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import "./stcaststreamer.css";

type GlyphProps = { size?: number; fill?: string };

const Icon = ({ d, size = 24, fill = "currentColor" }: GlyphProps & { d: string }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path d={d} fill={fill} />
  </svg>
);
const MicGlyph = (p: GlyphProps) => <Icon {...p} d="M12 14a3 3 0 0 0 3-3V5a3 3 0 0 0-6 0v6a3 3 0 0 0 3 3Zm5-3a5 5 0 0 1-10 0H5a7 7 0 0 0 6 6.9V21h2v-3.1A7 7 0 0 0 19 11h-2Z" />;
const VolumeGlyph = (p: GlyphProps) => <Icon {...p} d="M3 9v6h4l5 5V4L7 9H3Zm13.5 3A4.5 4.5 0 0 0 14 7.97v8.06A4.5 4.5 0 0 0 16.5 12ZM14 3.23v2.06a7 7 0 0 1 0 13.42v2.06a9 9 0 0 0 0-17.54Z" />;
const VolumeOffGlyph = (p: GlyphProps) => <Icon {...p} d="M3 9v6h4l5 5V4L7 9H3Zm13.5 3A4.5 4.5 0 0 0 14 7.97v2.21l2.45 2.45c.03-.2.05-.41.05-.63ZM19 12a7 7 0 0 1-1.39 4.2l1.45 1.45A8.96 8.96 0 0 0 21 12a9 9 0 0 0-7-8.77v2.06A7 7 0 0 1 19 12ZM4.27 3 3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25A6.94 6.94 0 0 1 14 18.7v2.06a8.92 8.92 0 0 0 3.69-1.81L19.73 21 21 19.73 12 10.73 4.27 3Z" />;
const VideocamGlyph = (p: GlyphProps) => <Icon {...p} d="M17 10.5V7a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-3.5l4 4v-11l-4 4Z" />;
const ScreenShareGlyph = (p: GlyphProps) => <Icon {...p} d="M20 18a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2H0v2h24v-2h-4Zm-7-3.53v-2.19c-2.78 0-4.61.85-6 2.72.56-2.67 2.11-5.33 6-5.87V7l4 3.73-4 3.74Z" />;
const ChatGlyph = (p: GlyphProps) => <Icon {...p} d="M20 2H4a2 2 0 0 0-2 2v18l4-4h14a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2Zm0 14H5.17L4 17.17V4h16v12Z" />;
const PeopleGlyph = (p: GlyphProps) => <Icon {...p} d="M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5s-3 1.34-3 3 1.34 3 3 3Zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5 5 6.34 5 8s1.34 3 3 3Zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5Zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5Z" />;
const CallEndGlyph = (p: GlyphProps) => <Icon {...p} d="M12 9c-1.6 0-3.15.25-4.6.72v3.1c0 .39-.23.74-.56.9-.98.49-1.87 1.12-2.66 1.85a.94.94 0 0 1-.69.27c-.27 0-.51-.11-.69-.28L.29 13.08a.956.956 0 0 1-.28-.68c0-.28.11-.52.29-.7C3.34 8.78 7.46 7 12 7s8.66 1.78 11.7 4.7c.18.18.3.42.3.7 0 .28-.11.52-.29.7l-2.48 2.48c-.18.17-.42.28-.69.28-.27 0-.51-.11-.69-.27a11.79 11.79 0 0 0-2.67-1.85.996.996 0 0 1-.56-.9v-3.1C15.15 9.25 13.6 9 12 9Z" />;
const ChevronGlyph = (p: GlyphProps) => <Icon {...p} d="M16.59 8.59 12 13.17 7.41 8.59 6 10l6 6 6-6-1.41-1.41Z" />;
const LiveDotGlyph = (p: GlyphProps) => <Icon {...p} d="M12 12m-8 0a8 8 0 1 0 16 0 8 8 0 1 0-16 0" />;
const RefreshGlyph = (p: GlyphProps) => <Icon {...p} d="M17.65 6.35A7.95 7.95 0 0 0 12 4a8 8 0 1 0 7.73 10h-2.08A6 6 0 1 1 12 6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35Z" />;
const CloseGlyph = (p: GlyphProps) => <Icon {...p} d="M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12 19 6.41Z" />;
const DiamondGlyph = () => (
  <svg viewBox="0 0 24 24" width="40" height="40" aria-hidden="true">
    <path d="M12 2 3 12l9 10 9-10L12 2Z" fill="var(--stcast-ruby)" />
  </svg>
);

function CastAvatar({ name = "Streamer", size = 100 }: { name?: string; size?: number }) {
  const hue = [...name].reduce((a, c) => a + c.charCodeAt(0), 0) % 360;
  return (
    <div
      className="stcast__avatar"
      style={{ "--sz": `${size}px`, "--hue": hue, fontSize: Math.max(10, Math.round(size * 0.45)) } as CSSProperties}
    >
      {(name[0] || "?").toUpperCase()}
    </div>
  );
}

type ToastAction = { label: string; onClick: () => void };
export type Toast = { id: string; title: string; message: string; action?: ToastAction };

function CastToast({
  title,
  message,
  action,
  onDismiss,
}: {
  title: string;
  message: string;
  action?: ToastAction;
  onDismiss?: () => void;
}) {
  return (
    <div className="stcast__toast" role="status">
      <div className="stcast__toastrow">
        <div className="stcast__toastbody">
          <div className="stcast__toasttitle">{title}</div>
          <div className="stcast__toastmsg">{message}</div>
        </div>
        <button type="button" className="stcast__toastclose" aria-label="Dismiss" onClick={onDismiss}>
          <CloseGlyph size={18} />
        </button>
      </div>
      {action && (
        <button type="button" className="stcast__toastaction" onClick={action.onClick}>
          {action.label}
        </button>
      )}
    </div>
  );
}

function DeviceRow({ glyph, label, tooltip }: { glyph: ReactNode; label: string; tooltip?: string }) {
  return (
    <div className="stcast__devrow u-tip">
      {glyph}
      <button type="button" className="stcast__devbtn" title={tooltip} aria-label={tooltip}>
        <span className="stcast__devlabel">{label}</span>
        <ChevronGlyph size={18} fill="currentColor" />
      </button>
      {tooltip && <span className="u-tip__bubble">{tooltip}</span>}
    </div>
  );
}

function CircleControl({ glyph, label, withChevron }: { glyph: ReactNode; label: string; withChevron?: boolean }) {
  return (
    <div className="stcast__cwm">
      <button type="button" className="stcast__circle" aria-label={label}>{glyph}</button>
      {withChevron && (
        <button type="button" className="stcast__chev" aria-label={`${label} devices`}>
          <ChevronGlyph size={20} fill="#fff" />
        </button>
      )}
    </div>
  );
}

type CastState = "onboarding" | "joining" | "live" | "error";

type StCastStreamerProps = {
  chrome?: boolean;
  state?: CastState;
  streamName?: string;
  displayName?: string;
  unreadMessages?: number;
  participants?: number;
  tabMuted?: boolean;
  toasts?: Toast[];
  errorTitle?: string;
  errorMessage?: string;
  onRetry?: () => void;
  onLeave?: () => void;
  retryLabel?: string;
  leaveLabel?: string;
};

export default function StCastStreamer({
  chrome = true,
  state = "live",
  streamName = "",
  displayName = "",
  unreadMessages = 0,
  participants = 0,
  tabMuted = false,
  toasts = [],
  errorTitle = "Failed to rejoin cast",
  errorMessage = "Please use the link for this Cast to rejoin.",
  onRetry = () => {},
  onLeave = () => {},
  retryLabel = "Reconnect",
  leaveLabel = "Leave",
}: StCastStreamerProps) {
  return (
    <SitesChromeMaybe chrome={chrome} active="legal">
      <div className={"stcast stcast--" + state}>
        {state === "onboarding" && (
          <div className="stcast__onboard">
            <div className="stcast__bg" aria-hidden="true" />
            <div className="stcast__modal" role="dialog" aria-label="Stream setup">
              <div className="stcast__logocircle">
                <DiamondGlyph />
              </div>
              <h1 className="stcast__title">
                {streamName ? `Casting to ${streamName}, Decentraland` : "Casting to Decentraland"}
              </h1>
              <p className="stcast__tag">Speaker</p>

              <input
                className="stcast__name"
                type="text"
                placeholder="Insert Your Name (Optional)"
                defaultValue={displayName}
              />

              <div className="stcast__devices">
                <DeviceRow glyph={<MicGlyph size={20} fill="currentColor" />} label="Default - Microphone" tooltip="Microphone" />
                <DeviceRow glyph={<VolumeGlyph size={20} fill="currentColor" />} label="Default - Speakers" tooltip="Speaker" />
                <DeviceRow glyph={<VideocamGlyph size={20} fill="currentColor" />} label="FaceTime HD Camera" tooltip="Camera" />
              </div>

              <button type="button" className="stcast__join">JOIN NOW</button>
            </div>
          </div>
        )}

        {state === "joining" && (
          <div className="stcast__onboard">
            <div className="stcast__bg" aria-hidden="true" />
            <div className="stcast__joining">
              <div className="stcast__joinlogo">
                <DiamondGlyph />
              </div>
              <div className="stcast__joiningtext">Joining...</div>
              <div className="stcast__spinner" aria-hidden="true" />
            </div>
          </div>
        )}

        {state === "live" && (
          <div className="stcast__view">
            <div className="stcast__main">
              <div className="stcast__videocontainer">
                <div className="stcast__videoarea">
                  <div className="stcast__livepill">
                    <LiveDotGlyph size={16} fill="#fff" />
                    LIVE
                  </div>

                  <div className="stcast__empty">
                    <div className="stcast__avatarpulse">
                      <CastAvatar name={displayName || "Streamer"} size={100} />
                    </div>
                    <div className="stcast__namepill">{displayName || "Streamer"}</div>
                  </div>
                </div>
              </div>

              <div className="stcast__controls">
                <div className="stcast__center">
                  <CircleControl glyph={<MicGlyph size={24} fill="#161518" />} label="Microphone" withChevron />
                  <CircleControl glyph={<VideocamGlyph size={24} fill="#161518" />} label="Camera" withChevron />
                  <CircleControl glyph={<ScreenShareGlyph size={24} fill="#161518" />} label="Share screen" withChevron />
                  <button type="button" className="stcast__leave">
                    <CallEndGlyph size={20} fill="#fff" />
                    LEAVE STREAM
                  </button>
                </div>

                <div className="stcast__right">
                  <button type="button" className="stcast__iconbtn" aria-label={tabMuted ? "Unmute Cast audio" : "Mute Cast audio"}>
                    {tabMuted ? <VolumeOffGlyph /> : <VolumeGlyph />}
                  </button>
                  <button type="button" className="stcast__iconbtn" aria-label="Chat">
                    <ChatGlyph />
                    {unreadMessages > 0 && <span className="stcast__badge">{unreadMessages}</span>}
                  </button>
                  <button type="button" className="stcast__iconbtn" aria-label="People">
                    <PeopleGlyph />
                    <span className="stcast__badge">{participants}</span>
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}

        {state === "error" && (
          <div className="stcast__overlay" role="alertdialog" aria-label={errorTitle}>
            <div className="stcast__errcard">
              <div className="stcast__erricon" aria-hidden="true">
                <RefreshGlyph size={40} fill="var(--stcast-ruby)" />
              </div>
              <div className="stcast__errtitle">{errorTitle}</div>
              <div className="stcast__errmsg">{errorMessage}</div>
              <div className="stcast__erractions">
                <button type="button" className="stcast__errbtn" onClick={onRetry}>
                  <RefreshGlyph size={18} fill="#fff" />
                  {retryLabel}
                </button>
                <button type="button" className="stcast__errbtn stcast__errbtn--ghost" onClick={onLeave}>
                  {leaveLabel}
                </button>
              </div>
            </div>
          </div>
        )}

        {toasts.length > 0 && (
          <div className="stcast__stack">
            {toasts.map((t) => (
              <CastToast key={t.id} title={t.title} message={t.message} action={t.action} onDismiss={() => {}} />
            ))}
          </div>
        )}
      </div>
    </SitesChromeMaybe>
  );
}
