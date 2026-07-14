import type { ReactNode } from "react";
import Toggle from "../../atoms/Toggle";
import Slider from "../../atoms/Slider";
import { Mute } from "../../atoms/icons";
import { useBridgeState, sendBridge } from "../../overlay/bridge";
import "./voicechat.css";

type RailItem = { key: string; node: ReactNode; active?: boolean; stroke?: boolean };

const RAIL_TOP: RailItem[] = [
  { key: "overflow", node: (<><circle cx="5" cy="12" r="1.9" /><circle cx="12" cy="12" r="1.9" /><circle cx="19" cy="12" r="1.9" /></>) },
];
const RAIL_ICONS: RailItem[] = [
  { key: "bell", node: <path d="M12 2a6 6 0 0 0-6 6c0 3.6-1 5.4-1.8 6.3-.5.6-.1 1.7.8 1.7h14c.9 0 1.3-1.1.8-1.7C19 13.4 18 11.6 18 8a6 6 0 0 0-6-6Zm0 20a2.6 2.6 0 0 0 2.6-2.4H9.4A2.6 2.6 0 0 0 12 22Z" /> },
  { key: "places", node: <path d="M12 2a7 7 0 0 0-7 7c0 5 7 13 7 13s7-8 7-13a7 7 0 0 0-7-7Zm0 9.5A2.5 2.5 0 1 1 12 6.5a2.5 2.5 0 0 1 0 5Z" /> },
  { key: "people", node: <path d="M8.5 11a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm7 0a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm-7 1.5C5.5 12.5 2 14 2 16.5V19h8v-2.5c0-1 .5-2 1.4-2.7a7 7 0 0 0-2.9-.8Zm7 0c-.6 0-1.2.1-1.8.2 1 .8 1.8 1.8 1.8 3.3V19h6v-2.5c0-2.5-3.5-4-6-4Z" /> },
  { key: "backpack", node: <path d="M8.2 5.4a3.8 3.8 0 0 1 7.6 0v.3A3.2 3.2 0 0 1 18.4 8.8V18a2.6 2.6 0 0 1-2.6 2.6H8.2A2.6 2.6 0 0 1 5.6 18V8.8A3.2 3.2 0 0 1 8.2 5.7v-.3Zm5.8.2V5.4a2 2 0 0 0-4 0v.2a3.2 3.2 0 0 1 .8-.1h2.4c.27 0 .54.04.8.1ZM10.2 16.6h3.6v2.2h-3.6v-2.2Z" /> },
  { key: "marketplace", node: <path d="M7 7V6a5 5 0 0 1 10 0v1h2.2l.8 12.5a2 2 0 0 1-2 2.1H6a2 2 0 0 1-2-2.1L4.8 7H7Zm2 0h6V6a3 3 0 0 0-6 0v1Z" /> },
  { key: "gallery", node: <path d="M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Zm0 2v9.6l3.8-3.8 2.6 2.6 3.4-3.4L19 16.8V6H5Zm3.5 1.8a1.7 1.7 0 1 1 0 3.4 1.7 1.7 0 0 1 0-3.4Z" /> },
  { key: "settings", node: <path d="M19.4 13a7.8 7.8 0 0 0 0-2l2-1.5-2-3.4-2.4 1a7.6 7.6 0 0 0-1.7-1l-.4-2.6H10l-.4 2.6c-.6.3-1.2.6-1.7 1l-2.4-1-2 3.4L3.6 11a7.8 7.8 0 0 0 0 2l-2 1.5 2 3.4 2.4-1c.5.4 1.1.7 1.7 1l.4 2.6h4l.4-2.6c.6-.3 1.2-.6 1.7-1l2.4 1 2-3.4-2-1.5ZM12 15.2A3.2 3.2 0 1 1 12 8.8a3.2 3.2 0 0 1 0 6.4Z" /> },
];
const RAIL_BOTTOM: RailItem[] = [
  { key: "skybox", stroke: true, node: (<><circle cx="12" cy="12" r="4" /><path d="M12 8a4 4 0 0 0 0 8Z" fill="currentColor" stroke="none" /><path d="M12 3v2.2M12 18.8V21M3 12h2.2M18.8 12H21M5.6 5.6l1.6 1.6M16.8 16.8l1.6 1.6M5.6 18.4l1.6-1.6M16.8 7.2l1.6-1.6" /></>) },
  { key: "camera", node: <path d="M9 4 7.5 6H5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-2.5L15 4H9Zm3 5a4.2 4.2 0 1 1 0 8.4A4.2 4.2 0 0 1 12 9Zm0 2a2.2 2.2 0 1 0 0 4.4A2.2 2.2 0 0 0 12 11Z" /> },
  { key: "emote", node: <path d="M13.5 4.2a1.7 1.7 0 1 1-3.4 0 1.7 1.7 0 0 1 3.4 0ZM10 7l-3.5 2 .8 1.7L9.5 9.4 9 12l-3 5.5 1.6 1L11 13l1.7 2.8L11 21h2l2-5-2-3.5.6-3 2.2 1.5 1-1.6L13.5 7H10Z" /> },
  { key: "chat", active: true, node: <path d="M4 5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H9l-4 3.5V16H6a2 2 0 0 1-2-2V5Z" /> },
];

function RailBtn({ item }: { item: RailItem }) {
  return (
    <span
      className={"vc__rail-btn" + (item.active ? " is-active" : "") + (item.stroke ? " is-stroke" : "")}
      aria-hidden="true"
    >
      <svg viewBox="0 0 24 24" width="22" height="22">{item.node}</svg>
    </span>
  );
}

export function VoicePanel({ className = "" }: { className?: string }) {
  const mic = useBridgeState((s) => s.mic);
  const micOn = !!mic?.enabled;
  const toggleMic = () => sendBridge("SetMic", { enabled: !micOn });
  return (
    <div className={"vc" + (className ? " " + className : "")}>
      <div className="vc__head">NEARBY VOICE</div>

      <div className="vc__row">
        <svg className="vc__icon" viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
          <path d="M8 9a4 4 0 1 1 8 0c0 2.3-2 3.2-2.6 4.6-.5 1.1-.4 2-.4 2.7 0 1.6-1.2 2.7-2.7 2.7-1.7 0-2.9-1.3-2.9-3" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"/>
          <path d="M9.8 9.5a2.2 2.2 0 0 1 4.2.9c0 1.3-1.5 1.7-1.9 2.7" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"/>
        </svg>
        <span className="vc__label">Microphone</span>
        <span className="vc__ctl"><Toggle checked={micOn} onChange={toggleMic} ariaLabel="Microphone" /></span>
      </div>

      <div className="vc__row vc__row--slider">
        <Mute size={22} className="vc__icon" />
        <Slider
          defaultValue={95}
          ariaLabel="Nearby voice volume"
          onChange={(v: number) => sendBridge("SetSetting", { name: "Voice Volume", value: v })}
        />
      </div>

      <button
        className={"vc__speak" + (micOn ? " is-active" : "")}
        type="button"
        aria-pressed={micOn}
        onClick={toggleMic}
      >
        <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
          <rect x="3.5" y="3.5" width="17" height="17" rx="5" fill="none" stroke="currentColor" strokeWidth="1.8"/>
          <circle cx="9" cy="10" r="1.2" fill="currentColor"/>
          <circle cx="15" cy="10" r="1.2" fill="currentColor"/>
          <path d="M9 14a3.2 3.2 0 0 0 6 0" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"/>
        </svg>
        <span>{micOn ? "Mic on \u{2014} click to mute" : "Speak"}</span>
      </button>

      <div className="vc__hint">Click <b>Microphone</b> to talk to people nearby</div>
    </div>
  );
}

export default function VoiceChat({ bare = false }: { bare?: boolean }) {
  if (bare) {
    return <VoicePanel className="vc--bare" />;
  }
  return (
    <div className="vc__stage">
      <div className="vc__rail" aria-hidden="true">
        <span className="vc__rail-cfg">
          <svg viewBox="0 0 24 24" width="22" height="22">{RAIL_TOP[0]?.node}</svg>
        </span>
        <span className="vc__rail-avatar" />
        <span className="vc__rail-divider" />
        {RAIL_ICONS.map((it) => (
          <RailBtn key={it.key} item={it} />
        ))}
        <span className="vc__rail-spacer" />
        {RAIL_BOTTOM.map((it) => (
          <RailBtn key={it.key} item={it} />
        ))}
      </div>

      <VoicePanel />

      <div className="vc__chat">
        <div className="vc__msg">
          <div className="vc__msg-name">
            DCL System
            <svg className="vc__msg-check" viewBox="0 0 24 24" width="12" height="12" aria-hidden="true">
              <path d="M5 12l4 4 10-10" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </div>
          <div className="vc__msg-body">Type <b>/help</b> for available commands.</div>
          <div className="vc__msg-time">03:28 PM</div>
        </div>
      </div>

      <div className="vc__input">
        <span className="vc__input-text">Press Enter to chat</span>
        <svg className="vc__input-heart" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
          <path d="M12 21s-7-4.3-9.3-8.2C1.2 10 2 6.5 5 5.5c2-.7 3.8.3 4.7 1.7L12 10l2.3-2.8c.9-1.4 2.7-2.4 4.7-1.7 3 1 3.8 4.5 2.3 7.3C19 16.7 12 21 12 21z" fill="currentColor"/>
        </svg>
      </div>
    </div>
  );
}
