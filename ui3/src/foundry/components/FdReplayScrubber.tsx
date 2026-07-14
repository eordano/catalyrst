import { useEffect } from "react";

import Slider from "../../atoms/Slider";
import "./fdreplayscrubber.css";

export type FdReplayScrubberProps = {
  cursor: number;
  maxSeq: number;
  onCursor: (seq: number) => void;
  onStep: (direction: "back" | "forward") => void;
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

/** The transport for the event ledger: slider, step buttons, and ←/→. */
export default function FdReplayScrubber({
  cursor,
  maxSeq,
  onCursor,
  onStep,
}: FdReplayScrubberProps) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.altKey || e.ctrlKey || e.metaKey) return;
      // A focused range input already moves on ←/→; stepping again would double it.
      if (typingTarget(e.target)) return;
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        onStep("back");
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        onStep("forward");
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onStep]);

  return (
    <div className="fd-scrub">
      <div className="fd-scrub__controls">
        <button
          type="button"
          className="fd-scrub__btn"
          onClick={() => onStep("back")}
          disabled={cursor <= 0}
          aria-label="Step back one event"
        >
          ← Step back
        </button>
        <button
          type="button"
          className="fd-scrub__btn"
          onClick={() => onStep("forward")}
          disabled={cursor >= maxSeq}
          aria-label="Step forward one event"
        >
          Step forward →
        </button>
        <span className="fd-scrub__pos">
          seq <strong>{cursor}</strong> of {maxSeq}
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

      <p className="fd-scrub__note">
        The ledger shows every event through the cursor and nothing after it.
        Arrow keys step one event at a time.
      </p>
    </div>
  );
}
