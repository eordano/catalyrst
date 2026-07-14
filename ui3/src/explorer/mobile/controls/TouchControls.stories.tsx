import { useCallback, useRef, useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import type { EngineInputLogEntry, LookStrategy } from "./engineInput";
import type { JoystickState } from "./VirtualJoystick";
import TouchControls from "./TouchControls";
import "./touchcontrols.stories.css";

const MAX_LOG = 40;

const PROBE_ID = "tc-probe-canvas";

type HarnessProps = {
  lookStrategy?: LookStrategy;
  restingKnob?: boolean;
  sprint?: boolean;
  walkBelowMagnitude?: number;
  jumpButton?: boolean;
};

function Harness({
  lookStrategy,
  restingKnob = true,
  sprint = true,
  walkBelowMagnitude = 0,
  jumpButton = true,
}: HarnessProps) {
  const [log, setLog] = useState<EngineInputLogEntry[]>([]);
  const [stick, setStick] = useState<JoystickState | null>(null);
  const [yaw, setYaw] = useState(0);
  const [pitch, setPitch] = useState(0);
  const [taps, setTaps] = useState(0);
  const heldRef = useRef<Set<string>>(new Set());
  const [held, setHeld] = useState<string[]>([]);

  const onEvent = useCallback((entry: EngineInputLogEntry) => {
    setLog((prev) => [entry, ...prev].slice(0, MAX_LOG));
    if (entry.type === "keydown") heldRef.current.add(entry.detail);
    if (entry.type === "keyup") heldRef.current.delete(entry.detail);
    setHeld([...heldRef.current]);
    if (entry.type === "pointermove" && entry.detail.startsWith("movement=")) {
      const parts = entry.detail.slice("movement=".length).split(",");
      const dx = Number(parts[0] ?? 0);
      const dy = Number(parts[1] ?? 0);
      setYaw((v) => v - dx * 0.1);
      setPitch((v) => Math.max(-89, Math.min(89, v - dy * 0.1)));
    }
  }, []);

  const props = {
    canvasId: PROBE_ID,
    restingKnob,
    sprint,
    walkBelowMagnitude,
    jumpButton,
    onEvent,
    onJoystickChange: setStick,
    onTap: () => setTaps((n) => n + 1),
    ...(lookStrategy ? { lookStrategy } : {}),
  };

  return (
    <div className="tcs">
      <div
        className="tcs__world"
        id={PROBE_ID}
        tabIndex={-1}
        style={{ backgroundPosition: `${yaw}px ${pitch}px` }}
      >
        <div className="tcs__hint">
          <strong>Placeholder world surface</strong>
          <span>
            Drag on the right side to look. Drag on the left column to move. Tap the right
            side to fire the primary action.
          </span>
        </div>
        <TouchControls {...props} />
      </div>
      <aside className="tcs__panel">
        <div className="tcs__row">
          <span className="tcs__label">held keys</span>
          <span className="tcs__value">{held.length ? held.join(" + ") : "none"}</span>
        </div>
        <div className="tcs__row">
          <span className="tcs__label">stick</span>
          <span className="tcs__value">
            {stick && stick.active
              ? `x ${stick.output.x.toFixed(2)} y ${stick.output.y.toFixed(2)} \u{B7} |v| ${stick.magnitude.toFixed(2)}${stick.sprinting ? " \u{B7} sprint" : ""}`
              : "idle"}
          </span>
        </div>
        <div className="tcs__row">
          <span className="tcs__label">taps</span>
          <span className="tcs__value">{taps}</span>
        </div>
        <ol className="tcs__log">
          {log.map((entry) => (
            <li key={entry.seq} data-type={entry.type}>
              <span className="tcs__seq">{entry.seq}</span>
              <span className="tcs__type">{entry.type}</span>
              <span className="tcs__detail">{entry.detail}</span>
            </li>
          ))}
        </ol>
        {log.length === 0 ? (
          <p className="tcs__empty">No synthesized events yet.</p>
        ) : null}
      </aside>
    </div>
  );
}

const meta = {
  title: "Explorer/Mobile/TouchControls",
  component: Harness,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Harness>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {},
};

export const ArrowKeyLook: Story = {
  args: { lookStrategy: "arrow-keys" },
};

export const HiddenUntilTouched: Story = {
  args: { restingKnob: false },
};

export const WalkBandAndNoSprint: Story = {
  args: { sprint: false, walkBelowMagnitude: 0.75 },
};
