import VoiceChat from "../../explorer/components/VoiceChat";
import Spinner from "../../atoms/Spinner";
import "./voicejoinview.css";

type VjKind = "private" | "community";

type VjSession = {
  roomId: string;
  roomName: string;
  peerName: string;
  pushToTalkKey: string;
};

type VoiceJoinViewProps = {
  value?: string;
  step?: string;
  session?: VjSession;
  kind?: VjKind;
  micMuted?: boolean;
  resultRoomName?: string;
  error?: string;
  onRequest?: (kind: VjKind) => void;
  onToggleMute?: () => void;
  onLeave?: () => void;
  onRetry?: () => void;
};

export default function VoiceJoinView({
  value = "resting",
  step = "voice",
  session = { roomId: "", roomName: "", peerName: "", pushToTalkKey: "" },
  kind = undefined,
  micMuted = false,
  resultRoomName = undefined,
  error = undefined,
  onRequest = undefined,
  onToggleMute = undefined,
  onLeave = undefined,
  onRetry = undefined,
}: VoiceJoinViewProps) {
  return (
    <div className="vj" data-step={step}>
      <VoiceChat />

      <aside className="vj__session" aria-label="Voice session" data-state={value}>
        <header className="vj__head">
          <span className={"vj__dot vj__dot--" + value} aria-hidden />
          <span className="vj__title">
            {value === "resting" && "Nearby voice \u{2014} mic off"}
            {value === "requesting" && "Requesting session\u{2026}"}
            {value === "connecting" && "Issuing token\u{2026}"}
            {value === "talking" && (micMuted ? "Muted" : "Talking")}
            {value === "left" && "Session ended"}
            {value === "failed" && "Couldn't connect"}
          </span>
          <span className="vj__stub" title="Connect + token are simulated">
            SIMULATED
          </span>
        </header>

        {value === "resting" && (
          <div className="vj__body">
            <p className="vj__hint">
              The mic is off. Start a voice session with{" "}
              <b>{session.peerName}</b>, or a community room.
            </p>
            <div className="vj__actions">
              <button
                type="button"
                className="vj__btn vj__btn--primary"
                onClick={() => onRequest?.("private")}
              >
                Start private call
              </button>
              <button
                type="button"
                className="vj__btn"
                onClick={() => onRequest?.("community")}
              >
                Join community voice
              </button>
            </div>
          </div>
        )}

        {(value === "requesting" || value === "connecting") && (
          <div className="vj__body" role="status">
            <Spinner size={26} color="#f5a623" aria-hidden />
            <p className="vj__hint">
              {value === "requesting"
                ? "Requesting a voice session\u{2026}"
                : "Issuing a LiveKit token and connecting\u{2026} (simulated)"}
            </p>
            <p className="vj__room">
              room: <code>voice-chat-{kind}-{session.roomId}</code>
            </p>
          </div>
        )}

        {value === "talking" && (
          <div className="vj__body">
            <p className="vj__hint">
              Connected to <code>{resultRoomName ?? session.roomName}</code>.{" "}
              {micMuted ? (
                "Your mic is muted."
              ) : (
                <>
                  Hold <b>[{session.pushToTalkKey}]</b> to push-to-talk.
                </>
              )}
            </p>
            <div className="vj__actions">
              <button
                type="button"
                className={"vj__btn" + (micMuted ? " vj__btn--primary" : "")}
                aria-pressed={micMuted}
                onClick={onToggleMute}
              >
                {micMuted ? "Unmute mic" : "Mute mic"}
              </button>
              <button
                type="button"
                className="vj__btn vj__btn--danger"
                onClick={onLeave}
              >
                Leave voice
              </button>
            </div>
          </div>
        )}

        {value === "failed" && (
          <div className="vj__body" role="alert">
            <p className="vj__hint">Connection failed: {error}</p>
            <div className="vj__actions">
              <button
                type="button"
                className="vj__btn vj__btn--primary"
                onClick={onRetry}
              >
                Retry
              </button>
              <button
                type="button"
                className="vj__btn"
                onClick={onLeave}
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {value === "left" && (
          <div className="vj__body" role="status">
            <div className="vj__check" aria-hidden>
              &#x2713;
            </div>
            <p className="vj__hint">
              You left the voice session. The room was torn down (simulated
              EndPrivateVoiceChat).
            </p>
          </div>
        )}
      </aside>
    </div>
  );
}
