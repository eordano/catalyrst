import type { CSSProperties } from "react";
import { Avatar } from "../../atoms/primitives";
import { asset } from "../../asset";
import "./communitystream.css";

const SIDEBAR: readonly (readonly [string, string, string?])[] = [
  ["backpack", "Backpack", "Explorer/Pages/Backpack"], ["emote", "Emotes", "Explorer/Components/EmoteWheel"], ["communities", "Communities", "Explorer/Pages/Communities"],
  ["friends", "Friends", "Explorer/Pages/Friends"], ["chat", "Chat", "Explorer/Frames/Chat"], ["credits", "Marketplace Credits"],
  ["wearables", "Smart Wearables", "Explorer/Components/SmartWearables"],
];
const SIDEBAR_LOWER: readonly (readonly [string, string, string?])[] = [
  ["notifications", "Notifications", "Explorer/Components/Notifications"], ["skybox", "Skybox", "Explorer/Components/SkyboxHUD"],
  ["settings", "Settings", "Explorer/Pages/Settings"], ["help", "Help"], ["controls", "Controls", "Explorer/Pages/Controls"],
];

function BroadcastIcon() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
      <circle cx="12" cy="12" r="2.2" fill="currentColor" stroke="none" />
      <path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4" />
      <path d="M5 5a10 10 0 0 0 0 14M19 5a10 10 0 0 1 0 14" />
    </svg>
  );
}

function PersonIcon() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="8" r="3.4" /><path d="M5.5 19a6.5 6.5 0 0 1 13 0" />
    </svg>
  );
}

function HangUpIcon() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor">
      <path d="M12 9c-2.4 0-4.7.4-6.9 1.2-.6.2-1 .8-1 1.5v2.1c0 .5.3 1 .8 1.2l2.4.9c.5.2 1.1 0 1.4-.4l1.2-1.6c1.4-.4 2.8-.4 4.2 0l1.2 1.6c.3.4.9.6 1.4.4l2.4-.9c.5-.2.8-.7.8-1.2v-2.1c0-.7-.4-1.3-1-1.5C16.7 9.4 14.4 9 12 9z" transform="rotate(135 12 12)" />
    </svg>
  );
}

export default function CommunityStream() {
  return (
    <div className="csm__stage" aria-label="Community voice stream (in world)">
      <div className="csm__world" aria-hidden="true" />

      <nav className="csm__sb" aria-label="HUD sidebar">
        <button className="csm__sb-more" title="Menu" aria-label="Menu">
          <span /><span /><span />
        </button>
        <button className="csm__sb-profile" title="Profile" aria-label="Profile" data-sb-linkto="Explorer/Pages/Passport">
          <Avatar hue={290} size={36} status="online" className="csm__sb-av" />
        </button>
        <div className="csm__sb-group">
          {SIDEBAR.map(([k, l, to]) => (
            <button key={k} className="csm__sb-btn" title={l} aria-label={l} data-sb-linkto={to || undefined}>
              <span className="csm__sb-icon u-mask-icon" style={{ "--i": `url(${asset("assets/nav/" + k + ".png")})` } as CSSProperties} />
            </button>
          ))}
        </div>
        <div className="csm__sb-spacer" />
        <div className="csm__sb-group">
          {SIDEBAR_LOWER.map(([k, l, to]) => (
            <button key={k} className="csm__sb-btn" title={l} aria-label={l} data-sb-linkto={to || undefined}>
              <span className="csm__sb-icon u-mask-icon" style={{ "--i": `url(${asset("assets/nav/" + k + ".png")})` } as CSSProperties} />
            </button>
          ))}
        </div>
      </nav>

      <div className="csm__mm">
        <div className="csm__mm-head">
          <div className="csm__mm-place">
            <span className="csm__mm-name u-truncate">interactive-text</span>
            <span className="csm__mm-coords">
              <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 21s7-5.7 7-11a7 7 0 1 0-14 0c0 5.3 7 11 7 11z" /><circle cx="12" cy="10" r="2.4" />
              </svg>
              140, 140
            </span>
          </div>
          <div className="csm__mm-actions">
            <button className="csm__mm-icbtn" aria-label="Favourite">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 20s-7-4.6-7-9.5A4.5 4.5 0 0 1 12 7a4.5 4.5 0 0 1 7 3.5C19 15.4 12 20 12 20z" />
              </svg>
            </button>
            <button className="csm__mm-icbtn" aria-label="Scene menu">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor">
                <circle cx="12" cy="5" r="1.7" /><circle cx="12" cy="12" r="1.7" /><circle cx="12" cy="19" r="1.7" />
              </svg>
            </button>
          </div>
        </div>
        <div className="csm__mm-map" aria-hidden="true" data-sb-linkto="Explorer/Pages/Map">
          <div className="csm__mm-grid" />
          <span className="csm__mm-c csm__mm-n">N</span>
          <span className="csm__mm-c csm__mm-e">E</span>
          <span className="csm__mm-c csm__mm-w">W</span>
          <span className="csm__mm-player" />
        </div>
      </div>

      <div className="csm__widget" role="region" aria-label="Community voice stream">
        <span className="csm__w-thumb" aria-hidden="true" />
        <div className="csm__w-body">
          <div className="csm__w-top">
            <span className="csm__w-live"><BroadcastIcon /></span>
            <span className="csm__w-count"><PersonIcon /> 10</span>
            <span className="csm__w-dot">•</span>
            <span className="csm__w-name u-truncate">Community Name</span>
          </div>
          <div className="csm__w-sub u-truncate">*Crickets*</div>
        </div>
        <button className="csm__w-hang" aria-label="Leave stream" title="Leave stream">
          <HangUpIcon />
        </button>
      </div>

      <div className="csm__chat">
        <div className="csm__chat-day">Today</div>
        <div className="csm__chat-msg">
          <div className="csm__chat-from">
            DCL System
            <svg className="csm__chat-verif" viewBox="0 0 24 24" width="13" height="13" fill="currentColor" aria-hidden="true">
              <path d="M12 2l2.4 1.7 2.9-.3 1 2.8 2.5 1.5-.7 2.8 1 2.8-2.5 1.5-1 2.8-2.9-.3L12 22l-2.4-1.7-2.9.3-1-2.8L3.2 16l.7-2.8-1-2.8 2.5-1.5 1-2.8 2.9.3L12 2z" />
            </svg>
          </div>
          <div className="csm__chat-text">Type /help for available commands.</div>
          <div className="csm__chat-time">03:28 PM</div>
        </div>
      </div>

      <div className="csm__input">
        <Avatar hue={320} size={26} className="csm__input-av" />
        <span className="csm__input-text">Press Enter to chat</span>
        <button className="csm__input-react" aria-label="React" data-sb-linkto="Explorer/Components/ChatReactions">
          <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor">
            <path d="M12 20s-7-4.6-7-9.5A4.5 4.5 0 0 1 12 7a4.5 4.5 0 0 1 7 3.5C19 15.4 12 20 12 20z" />
          </svg>
        </button>
      </div>
    </div>
  );
}
