import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";

import Spinner from "../../atoms/Spinner";
import { useBridgeState } from "../../overlay/bridge";
import "./jumploading.css";

// Fallback for jumps the engine never reports as loading (the destination scene is
// already resident, so the deduped loading push stays ready=true and silent).
const INSTANT_JUMP_FALLBACK_MS = 3500;

// Ceiling on the quiet wait. A destination that still is not ready by now gets a
// warning card instead of a fake success: the user chooses to enter anyway or to
// stay where they were.
const JUMP_MAX_MS = 30000;

// Tiny store, not a bare module flag: AppLayout renders from this value, so it needs
// a subscription that re-renders it the moment a panel jump begins or ends.
let panelJumpActive = false;
const panelJumpListeners = new Set<() => void>();

function setPanelJumpActive(next: boolean): void {
  if (panelJumpActive === next) return;
  panelJumpActive = next;
  for (const l of panelJumpListeners) l();
}

function subscribePanelJump(cb: () => void): () => void {
  panelJumpListeners.add(cb);
  return () => {
    panelJumpListeners.delete(cb);
  };
}

export function isPanelJumpActive(): boolean {
  return panelJumpActive;
}

export function usePanelJumpActive(): boolean {
  return useSyncExternalStore(subscribePanelJump, isPanelJumpActive, () => false);
}

export function useJump(onDone?: () => void): {
  jumping: string | null;
  stalled: boolean;
  beginJump: (name: string) => void;
  cancelJump: () => void;
  confirmJump: () => void;
} {
  const [jumping, setJumping] = useState<string | null>(null);
  const [stalled, setStalled] = useState(false);
  const loading = useBridgeState((s) => s.loading);
  const sawLoadingRef = useRef(false);
  const timerRef = useRef<number | undefined>(undefined);
  const maxTimerRef = useRef<number | undefined>(undefined);
  const doneRef = useRef(onDone);
  doneRef.current = onDone;

  const clearTimers = useCallback(() => {
    if (timerRef.current !== undefined) window.clearTimeout(timerRef.current);
    timerRef.current = undefined;
    if (maxTimerRef.current !== undefined) window.clearTimeout(maxTimerRef.current);
    maxTimerRef.current = undefined;
  }, []);

  const finish = useCallback(() => {
    clearTimers();
    setPanelJumpActive(false);
    setJumping(null);
    setStalled(false);
    doneRef.current?.();
  }, [clearTimers]);

  // Dismiss without the success path: the overlay comes down and the user stays
  // on the panel they were on. The teleport request itself is not recalled.
  const cancelJump = useCallback(() => {
    clearTimers();
    setPanelJumpActive(false);
    setJumping(null);
    setStalled(false);
  }, [clearTimers]);

  const confirmJump = finish;

  const beginJump = useCallback((name: string) => {
    setPanelJumpActive(true);
    sawLoadingRef.current = false;
    setJumping(name || "destination");
    setStalled(false);
    if (timerRef.current !== undefined) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      if (!sawLoadingRef.current) finish();
    }, INSTANT_JUMP_FALLBACK_MS);
    if (maxTimerRef.current !== undefined) window.clearTimeout(maxTimerRef.current);
    maxTimerRef.current = window.setTimeout(() => setStalled(true), JUMP_MAX_MS);
  }, [finish]);

  useEffect(() => {
    if (jumping == null || !loading) return;
    if (!loading.ready) {
      sawLoadingRef.current = true;
      if (timerRef.current !== undefined) {
        window.clearTimeout(timerRef.current);
        timerRef.current = undefined;
      }
      return;
    }
    if (sawLoadingRef.current) finish();
  }, [jumping, loading, finish]);

  useEffect(
    () => () => {
      if (timerRef.current !== undefined) window.clearTimeout(timerRef.current);
      if (maxTimerRef.current !== undefined) window.clearTimeout(maxTimerRef.current);
      setPanelJumpActive(false);
    },
    [],
  );

  return { jumping, stalled, beginJump, cancelJump, confirmJump };
}

type JumpLoadingProps = {
  name?: string;
  stalled?: boolean;
  onCancel?: () => void;
  onEnterAnyway?: () => void;
};

export default function JumpLoading({
  name,
  stalled = false,
  onCancel,
  onEnterAnyway,
}: JumpLoadingProps) {
  const cancelRef = useRef(onCancel);
  cancelRef.current = onCancel;

  // Capture-phase so the cancel beats the panel-level Escape handlers; AppLayout
  // yields Escape while a jump overlay is up.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !cancelRef.current) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      cancelRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  if (stalled) {
    return (
      <div className="jl" role="alertdialog" aria-label="Teleport is taking too long">
        <div className="jl__text">This scene is taking too long&#x2026; enter anyway?</div>
        <div className="jl__actions">
          {onEnterAnyway && (
            <button type="button" className="jl__btn jl__btn--primary" onClick={onEnterAnyway}>
              Enter anyway
            </button>
          )}
          {onCancel && (
            <button type="button" className="jl__btn" onClick={onCancel}>
              Cancel
            </button>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="jl" role="status" aria-live="polite">
      <Spinner size={54} aria-hidden />
      <div className="jl__text">Teleporting{name ? ` to ${name}` : ""}&#x2026;</div>
      {onCancel && (
        <button type="button" className="jl__btn" onClick={onCancel}>
          Cancel
        </button>
      )}
    </div>
  );
}
