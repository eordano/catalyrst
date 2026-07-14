import Button from "../../atoms/Button";
import Slider from "../../atoms/Slider";
import "./fdreplayscrubber.css";

export type FdReplayScrubberProps = {
  cursor: number;
  maxSeq: number;
  onCursor: (seq: number) => void;
  onStep: (direction: "back" | "forward") => void;
  /** Playback state: when provided, a Play/Pause control auto-advances the
   *  cursor through the recorded events — nothing beyond the log is shown. */
  playing?: boolean;
  onPlayToggle?: () => void;
};

function typingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    target.isContentEditable
  );
}

/** The transport for the events: slider, step buttons, and ←/→ while the
 *  transport holds focus. */
export default function FdReplayScrubber({
  cursor,
  maxSeq,
  onCursor,
  onStep,
  playing = false,
  onPlayToggle,
}: FdReplayScrubberProps) {
  return (
    <div
      className="fd-scrub"
      onKeyDown={(e) => {
        if (e.altKey || e.ctrlKey || e.metaKey) return;
        // A focused range input already moves on ←/→; stepping again doubles it.
        if (typingTarget(e.target)) return;
        if (e.key === "ArrowLeft") {
          e.preventDefault();
          onStep("back");
        } else if (e.key === "ArrowRight") {
          e.preventDefault();
          onStep("forward");
        }
      }}
    >
      <div className="fd-scrub__controls">
        {onPlayToggle ? (
          <Button
            variant="secondary"
            size="sm"
            onClick={onPlayToggle}
            aria-label={playing ? "Pause playback" : "Play the events forward"}
          >
            {playing ? "Pause" : "Play"}
          </Button>
        ) : null}
        <Button
          variant="secondary"
          size="sm"
          onClick={() => onStep("back")}
          disabled={cursor <= 0}
          aria-label="Step back one event"
        >
          ← Step back
        </Button>
        <Button
          variant="secondary"
          size="sm"
          onClick={() => onStep("forward")}
          disabled={cursor >= maxSeq}
          aria-label="Step forward one event"
        >
          Step forward →
        </Button>
        <span className="fd-scrub__pos">
          <strong>#{cursor}</strong> of #{maxSeq}
        </span>
      </div>

      <Slider
        value={cursor}
        min={0}
        max={maxSeq}
        step={1}
        onChange={onCursor}
        ariaLabel="Replay position, in event sequence numbers"
        format={(v) => `#${v}`}
      />

      <p className="fd-scrub__note">Arrow keys step one event at a time.</p>
    </div>
  );
}
