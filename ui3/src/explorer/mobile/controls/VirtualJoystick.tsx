import { useCallback, useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import type { EngineInput, MovementCode } from "./engineInput";
import { capturePointer, releasePointer } from "./pointerCapture";
import type { JoystickMode, Vec2 } from "./joystickGeometry";
import {
  AXIS_THRESHOLD,
  CHAIN_LINKS,
  CLAMP_RADIUS,
  FIXED_BASE_RADIUS,
  MOVEMENT_DEADZONE,
  REVEAL_DELAY_MS,
  SPRINT_DWELL_MS,
  chainLength,
  chainLink,
  designScale,
  resolveJoystick,
  restingBase,
} from "./joystickGeometry";

export type JoystickState = {
  active: boolean;
  output: Vec2;
  magnitude: number;
  keys: MovementCode[];
  sprinting: boolean;
};

export type VirtualJoystickProps = {
  input: EngineInput;
  mode?: JoystickMode;
  deadzone?: number;
  axisThreshold?: number;
  sprint?: boolean;
  walkBelowMagnitude?: number;
  restingKnob?: boolean;
  disabled?: boolean;
  className?: string;
  onChange?: (state: JoystickState) => void;
};

const IDLE: JoystickState = {
  active: false,
  output: { x: 0, y: 0 },
  magnitude: 0,
  keys: [],
  sprinting: false,
};

const LINKS = Array.from({ length: CHAIN_LINKS }, (_, i) => i);

export default function VirtualJoystick({
  input,
  mode = "dynamic",
  deadzone = MOVEMENT_DEADZONE,
  axisThreshold = AXIS_THRESHOLD,
  sprint = true,
  walkBelowMagnitude = 0,
  restingKnob = true,
  disabled = false,
  className,
  onChange,
}: VirtualJoystickProps) {
  const zoneRef = useRef<HTMLDivElement>(null);
  const pointerRef = useRef<number | null>(null);
  const baseRef = useRef<Vec2>({ x: 0, y: 0 });
  const tipRef = useRef<Vec2>({ x: 0, y: 0 });
  const scaleRef = useRef(1);
  const chainRef = useRef(1);
  const gestureRef = useRef(0);
  const revealRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const sprintRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const sprintingRef = useRef(false);
  const walkingRef = useRef(false);
  const [visible, setVisible] = useState(false);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const paint = useCallback(() => {
    const zone = zoneRef.current;
    if (!zone) return;
    const base = baseRef.current;
    const tip = tipRef.current;
    zone.style.setProperty("--tc-base-x", `${base.x}px`);
    zone.style.setProperty("--tc-base-y", `${base.y}px`);
    zone.style.setProperty("--tc-tip-x", `${tip.x}px`);
    zone.style.setProperty("--tc-tip-y", `${tip.y}px`);
    const maxChain = chainRef.current;
    const distance = Math.min(1, Math.hypot(tip.x, tip.y) / maxChain);
    for (const index of LINKS) {
      const link = chainLink(index, distance, maxChain);
      const t = (index + 1) / (CHAIN_LINKS + 1);
      zone.style.setProperty(`--tc-link-${index}-x`, `${tip.x * t}px`);
      zone.style.setProperty(`--tc-link-${index}-y`, `${tip.y * t}px`);
      zone.style.setProperty(`--tc-link-${index}-r`, `${link.radius}`);
      zone.style.setProperty(`--tc-link-${index}-a`, `${link.alpha}`);
    }
  }, []);

  const measure = useCallback(() => {
    const zone = zoneRef.current;
    if (!zone) return;
    const scale = designScale(window.innerWidth, window.innerHeight);
    scaleRef.current = scale;
    chainRef.current = Math.max(1, chainLength(zone.clientWidth, zone.clientHeight));
    zone.style.setProperty("--tc-scale", `${scale}`);
    if (pointerRef.current === null) {
      baseRef.current = restingBase(zone.clientHeight, scale);
      tipRef.current = { x: 0, y: 0 };
    }
    paint();
  }, [paint]);

  const clearTimers = useCallback(() => {
    if (revealRef.current !== null) {
      clearTimeout(revealRef.current);
      revealRef.current = null;
    }
    if (sprintRef.current !== null) {
      clearTimeout(sprintRef.current);
      sprintRef.current = null;
    }
  }, []);

  const reset = useCallback(() => {
    clearTimers();
    pointerRef.current = null;
    sprintingRef.current = false;
    walkingRef.current = false;
    input.setMovement([]);
    input.setModifier("ShiftLeft", false);
    input.setModifier("ControlLeft", false);
    setVisible(false);
    measure();
    onChangeRef.current?.(IDLE);
  }, [clearTimers, input, measure]);

  useEffect(() => {
    measure();
    window.addEventListener("resize", measure);
    window.addEventListener("orientationchange", measure);
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("orientationchange", measure);
    };
  }, [measure]);

  useEffect(() => reset, [reset]);

  useEffect(() => {
    if (disabled) reset();
  }, [disabled, reset]);

  const apply = useCallback(
    (localX: number, localY: number) => {
      const base = baseRef.current;
      const vector = { x: localX - base.x, y: localY - base.y };
      tipRef.current = vector;
      const radius = CLAMP_RADIUS * scaleRef.current;
      const resolved = resolveJoystick(vector, radius, deadzone, axisThreshold);
      input.setMovement(resolved.keys);

      const wantsWalk =
        walkBelowMagnitude > 0 &&
        resolved.keys.length > 0 &&
        resolved.magnitude < walkBelowMagnitude;
      if (wantsWalk !== walkingRef.current) {
        walkingRef.current = wantsWalk;
        input.setModifier("ControlLeft", wantsWalk);
      }

      if (!sprint || !resolved.sprintEligible) {
        if (sprintRef.current !== null) {
          clearTimeout(sprintRef.current);
          sprintRef.current = null;
        }
        if (sprintingRef.current) {
          sprintingRef.current = false;
          input.setModifier("ShiftLeft", false);
        }
      } else if (sprintRef.current === null && !sprintingRef.current) {
        sprintRef.current = setTimeout(() => {
          sprintRef.current = null;
          sprintingRef.current = true;
          input.setModifier("ShiftLeft", true);
        }, SPRINT_DWELL_MS);
      }

      paint();
      onChangeRef.current?.({
        active: true,
        output: resolved.output,
        magnitude: resolved.magnitude,
        keys: resolved.keys,
        sprinting: sprintingRef.current,
      });
    },
    [axisThreshold, deadzone, input, paint, sprint, walkBelowMagnitude],
  );

  function onPointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    if (disabled || pointerRef.current !== null) return;
    const zone = zoneRef.current;
    if (!zone) return;
    const rect = zone.getBoundingClientRect();
    const localX = event.clientX - rect.left;
    const localY = event.clientY - rect.top;
    if (mode === "fixed") {
      const base = baseRef.current;
      const reach = FIXED_BASE_RADIUS * scaleRef.current;
      if (Math.hypot(localX - base.x, localY - base.y) > reach) return;
    } else {
      baseRef.current = { x: localX, y: localY };
    }
    event.preventDefault();
    pointerRef.current = event.pointerId;
    capturePointer(zone, event.pointerId);
    input.focusCanvas();
    gestureRef.current += 1;
    const gesture = gestureRef.current;
    clearTimers();
    revealRef.current = setTimeout(() => {
      revealRef.current = null;
      if (gestureRef.current === gesture && pointerRef.current !== null) setVisible(true);
    }, REVEAL_DELAY_MS);
    apply(localX, localY);
  }

  function onPointerMove(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.pointerId !== pointerRef.current) return;
    const zone = zoneRef.current;
    if (!zone) return;
    const rect = zone.getBoundingClientRect();
    apply(event.clientX - rect.left, event.clientY - rect.top);
  }

  function onPointerEnd(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.pointerId !== pointerRef.current) return;
    releasePointer(zoneRef.current, event.pointerId);
    reset();
  }

  const stage = visible ? "active" : restingKnob ? "resting" : "hidden";

  return (
    <div
      ref={zoneRef}
      className={className ? `tc__stick ${className}` : "tc__stick"}
      data-stage={stage}
      data-disabled={disabled ? "true" : "false"}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerEnd}
      onPointerCancel={onPointerEnd}
      onLostPointerCapture={onPointerEnd}
    >
      <div className="tc__knob">
        <div className="tc__ring" />
        <div className="tc__core" />
        {LINKS.map((index) => (
          <div key={index} className="tc__link" data-index={index} />
        ))}
        <div className="tc__tip">
          <div className="tc__tip-core" />
        </div>
      </div>
    </div>
  );
}
