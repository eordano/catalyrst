import { useEffect } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import type {
  EngineInputLogEntry,
  EngineInputOptions,
  LookStrategy,
} from "./engineInput";
import type { JoystickMode } from "./joystickGeometry";
import type { JoystickState } from "./VirtualJoystick";
import LookSurface from "./LookSurface";
import { capturePointer, releasePointer } from "./pointerCapture";
import VirtualJoystick from "./VirtualJoystick";
import { useEngineInput } from "./useEngineInput";
import "./touchcontrols.css";

export type TouchControlsProps = {
  canvasId?: string;
  target?: HTMLElement | null;
  enabled?: boolean;
  mode?: JoystickMode;
  lookStrategy?: LookStrategy;
  lookSensitivity?: number;
  lookInvertY?: boolean;
  deadzone?: number;
  axisThreshold?: number;
  sprint?: boolean;
  walkBelowMagnitude?: number;
  restingKnob?: boolean;
  jumpButton?: boolean;
  tapToInteract?: boolean;
  tapMaxMs?: number;
  tapSlopPx?: number;
  className?: string;
  onEvent?: (entry: EngineInputLogEntry) => void;
  onJoystickChange?: (state: JoystickState) => void;
  onTap?: (clientX: number, clientY: number) => void;
};

export default function TouchControls({
  canvasId,
  target,
  enabled = true,
  mode,
  lookStrategy,
  lookSensitivity,
  lookInvertY,
  deadzone,
  axisThreshold,
  sprint,
  walkBelowMagnitude,
  restingKnob,
  jumpButton = true,
  tapToInteract,
  tapMaxMs,
  tapSlopPx,
  className,
  onEvent,
  onJoystickChange,
  onTap,
}: TouchControlsProps) {
  const options: EngineInputOptions = {};
  if (canvasId !== undefined) options.canvasId = canvasId;
  if (target !== undefined) options.target = target;
  if (lookStrategy !== undefined) options.lookStrategy = lookStrategy;
  if (lookSensitivity !== undefined) options.lookSensitivity = lookSensitivity;
  if (lookInvertY !== undefined) options.lookInvertY = lookInvertY;
  options.onEvent = onEvent ?? null;

  const input = useEngineInput(options);

  useEffect(() => {
    if (!enabled) input.releaseAll();
  }, [enabled, input]);

  function onJumpDown(event: ReactPointerEvent<HTMLButtonElement>) {
    event.preventDefault();
    capturePointer(event.currentTarget, event.pointerId);
    input.focusCanvas();
    input.setAction("Space", true);
  }

  function onJumpUp(event: ReactPointerEvent<HTMLButtonElement>) {
    releasePointer(event.currentTarget, event.pointerId);
    input.setAction("Space", false);
  }

  return (
    <div
      className={className ? `tc ${className}` : "tc"}
      data-enabled={enabled ? "true" : "false"}
    >
      <LookSurface
        input={input}
        disabled={!enabled}
        {...(tapToInteract !== undefined ? { tapToInteract } : {})}
        {...(tapMaxMs !== undefined ? { tapMaxMs } : {})}
        {...(tapSlopPx !== undefined ? { tapSlopPx } : {})}
        {...(onTap !== undefined ? { onTap } : {})}
      />
      <VirtualJoystick
        input={input}
        disabled={!enabled}
        {...(mode !== undefined ? { mode } : {})}
        {...(deadzone !== undefined ? { deadzone } : {})}
        {...(axisThreshold !== undefined ? { axisThreshold } : {})}
        {...(sprint !== undefined ? { sprint } : {})}
        {...(walkBelowMagnitude !== undefined ? { walkBelowMagnitude } : {})}
        {...(restingKnob !== undefined ? { restingKnob } : {})}
        {...(onJoystickChange !== undefined ? { onChange: onJoystickChange } : {})}
      />
      {jumpButton ? (
        <button
          type="button"
          className="tc__jump"
          aria-label="Jump"
          onPointerDown={onJumpDown}
          onPointerUp={onJumpUp}
          onPointerCancel={onJumpUp}
          onLostPointerCapture={onJumpUp}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            <path
              d="M12 4l6 7h-4v9h-4v-9H6z"
              fill="currentColor"
            />
          </svg>
        </button>
      ) : null}
    </div>
  );
}
