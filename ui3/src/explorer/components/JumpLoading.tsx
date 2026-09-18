import { createContext, useContext, useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";

import JumpProgress from "./JumpProgress";
import { useLoadingTransfers } from "../../overlay/loadingTransfers";
import { useBridgeState } from "../../overlay/bridge";
import "./jumploading.css";

export const JumpCompleteContext = createContext<(() => void) | undefined>(undefined);

const JUMP_MAX_MS = 30000;

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

function isPanelJumpActive(): boolean {
  return panelJumpActive;
}

export function usePanelJumpActive(): boolean {
  return useSyncExternalStore(subscribePanelJump, isPanelJumpActive, () => false);
}

export function useJump(onDone?: () => void): {
  jumping: string | null;
  stalled: boolean;
  beginJump: (name: string, targetParcel?: string) => void;
  cancelJump: () => void;
  confirmJump: () => void;
} {
  const [jumping, setJumping] = useState<string | null>(null);
  const [stalled, setStalled] = useState(false);
  const loading = useBridgeState((s) => s.loading);
  const transfers = useLoadingTransfers(jumping != null);
  const scene = useBridgeState((s) => s.scene);
  const origin = useRef(scene);
  const sawLoadingRef = useRef(false);
  const maxTimerRef = useRef<number | undefined>(undefined);
  const onComplete = useContext(JumpCompleteContext);
  const completeRef = useRef(onComplete);
  completeRef.current = onComplete;
  const doneRef = useRef(onDone);
  doneRef.current = onDone;

  const clearTimers = useCallback(() => {
    if (maxTimerRef.current !== undefined) window.clearTimeout(maxTimerRef.current);
    maxTimerRef.current = undefined;
  }, []);

  const finish = useCallback(() => {
    clearTimers();
    setPanelJumpActive(false);
    setJumping(null);
    setStalled(false);
    doneRef.current?.();
    completeRef.current?.();
  }, [clearTimers]);

  const cancelJump = useCallback(() => {
    clearTimers();
    setPanelJumpActive(false);
    setJumping(null);
    setStalled(false);
  }, [clearTimers]);

  const confirmJump = finish;

  const beginJump = useCallback((name: string, targetParcel?: string) => {
    if (targetParcel && scene.coords === targetParcel && loading?.ready) { finish(); return; }
    origin.current = scene;
    setPanelJumpActive(true);
    sawLoadingRef.current = false;
    setJumping(name || "destination");
    setStalled(false);
    if (maxTimerRef.current !== undefined) window.clearTimeout(maxTimerRef.current);
    maxTimerRef.current = window.setTimeout(() => setStalled(true), JUMP_MAX_MS);
  }, [scene, loading?.ready, finish]);

  useEffect(() => {
    if (jumping == null) return;
    setStalled(false);
    clearTimers();
    maxTimerRef.current = window.setTimeout(() => setStalled(true), JUMP_MAX_MS);
    return clearTimers;
  }, [jumping, loading?.percent, loading?.pendingAssets, transfers.receivedBytes, transfers.completed, clearTimers]);

  useEffect(() => {
    if (jumping == null || !loading) return;
    if (!loading.ready) {
      sawLoadingRef.current = true;
      return;
    }
    const moved = scene.realm !== origin.current.realm || scene.coords !== origin.current.coords;
    if (sawLoadingRef.current || moved) finish();
  }, [jumping, loading, scene, finish]);

  useEffect(
    () => () => {
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

  return (
    <div className="jl" role={stalled ? "alertdialog" : undefined} aria-label={stalled ? "Teleport is taking too long" : undefined}>
      <div className="jl__text" role="status">Teleporting{name ? ` to ${name}` : ""}&#x2026;</div>
      <JumpProgress />
      {stalled && <p className="jl__stalled">No loading progress for 30 seconds. Keep waiting or enter anyway.</p>}
      <div className="jl__actions">
        {stalled && onEnterAnyway && <button type="button" className="jl__btn jl__btn--primary" onClick={onEnterAnyway}>Enter anyway</button>}
        {onCancel && <button type="button" className="jl__btn" onClick={onCancel}>Cancel</button>}
      </div>
    </div>
  );
}
