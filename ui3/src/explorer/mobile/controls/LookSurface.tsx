import { useCallback, useEffect, useRef } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import type { EngineInput } from "./engineInput";
import { capturePointer, releasePointer } from "./pointerCapture";

export type LookSurfaceProps = {
  input: EngineInput;
  tapMaxMs?: number;
  tapSlopPx?: number;
  tapToInteract?: boolean;
  disabled?: boolean;
  className?: string;
  onTap?: (clientX: number, clientY: number) => void;
  onLookStart?: () => void;
  onLookEnd?: () => void;
};

type Gesture = {
  pointerId: number;
  startX: number;
  startY: number;
  lastX: number;
  lastY: number;
  startedAt: number;
  travelled: number;
  looking: boolean;
};

export default function LookSurface({
  input,
  tapMaxMs = 250,
  tapSlopPx = 10,
  tapToInteract = true,
  disabled = false,
  className,
  onTap,
  onLookStart,
  onLookEnd,
}: LookSurfaceProps) {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const gestureRef = useRef<Gesture | null>(null);
  const activeRef = useRef(new Set<number>());
  const blockedRef = useRef(false);
  const onTapRef = useRef(onTap);
  const onLookStartRef = useRef(onLookStart);
  const onLookEndRef = useRef(onLookEnd);
  onTapRef.current = onTap;
  onLookStartRef.current = onLookStart;
  onLookEndRef.current = onLookEnd;

  const stopLook = useCallback(() => {
    const gesture = gestureRef.current;
    gestureRef.current = null;
    if (gesture?.looking) {
      input.endLook();
      onLookEndRef.current?.();
    }
  }, [input]);

  const reset = useCallback(() => {
    stopLook();
    activeRef.current.clear();
    blockedRef.current = false;
  }, [stopLook]);

  useEffect(() => reset, [reset]);

  useEffect(() => {
    if (disabled) reset();
  }, [disabled, reset]);

  function onPointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    if (disabled) return;
    const surface = surfaceRef.current;
    if (!surface) return;
    activeRef.current.add(event.pointerId);
    event.preventDefault();
    capturePointer(surface, event.pointerId);
    input.focusCanvas();

    if (activeRef.current.size >= 2) {
      blockedRef.current = true;
      stopLook();
      return;
    }
    if (blockedRef.current) return;

    gestureRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      lastX: event.clientX,
      lastY: event.clientY,
      startedAt: Date.now(),
      travelled: 0,
      looking: false,
    };
  }

  function onPointerMove(event: ReactPointerEvent<HTMLDivElement>) {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId || blockedRef.current) return;
    const dx = event.clientX - gesture.lastX;
    const dy = event.clientY - gesture.lastY;
    gesture.lastX = event.clientX;
    gesture.lastY = event.clientY;
    gesture.travelled = Math.hypot(
      event.clientX - gesture.startX,
      event.clientY - gesture.startY,
    );
    if (!gesture.looking) {
      if (gesture.travelled < tapSlopPx) return;
      gesture.looking = true;
      input.beginLook(gesture.startX, gesture.startY);
      onLookStartRef.current?.();
    }
    input.look(dx, dy);
  }

  function onPointerEnd(event: ReactPointerEvent<HTMLDivElement>) {
    releasePointer(surfaceRef.current, event.pointerId);
    activeRef.current.delete(event.pointerId);
    if (activeRef.current.size === 0) blockedRef.current = false;

    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    const tapped =
      event.type === "pointerup" &&
      !gesture.looking &&
      gesture.travelled < tapSlopPx &&
      Date.now() - gesture.startedAt <= tapMaxMs;
    stopLook();
    if (!tapped) return;
    if (tapToInteract) input.primaryTap(event.clientX, event.clientY);
    onTapRef.current?.(event.clientX, event.clientY);
  }

  return (
    <div
      ref={surfaceRef}
      className={className ? `tc__look ${className}` : "tc__look"}
      data-disabled={disabled ? "true" : "false"}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerEnd}
      onPointerCancel={onPointerEnd}
      onLostPointerCapture={onPointerEnd}
    />
  );
}
