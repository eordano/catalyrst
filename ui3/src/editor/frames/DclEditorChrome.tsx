import type { ReactNode, RefObject } from "react";
import { useEffect, useReducer, useRef, useState } from "react";
import EngineViewport from "./EngineViewport";
import MobileEditorGate from "../../components/MobileEditorGate";
import { BOOT_TIMEOUT_MS, BOOT_PROGRESS_POLL_MS, BOOT_LEAVE_MS } from "../editor-config";
import { bootReducer, bootOverlay, INITIAL_BOOT, type BootStage } from "../boot-machine";
import "./dcleditorchrome.css";
import "./dcleditorchrome-toolbar.css";
import "./dcleditorchrome-menus.css";
import "./dcleditorchrome-hierarchy.css";
import "./dcleditorchrome-inspector.css";
import "./dcleditorchrome-shell.css";
import "./dcleditorchrome-assets.css";
import "./dcleditorchrome-overlays.css";

const BOOT_STAGES: readonly BootStage[] = ["download", "compile", "init", "workers", "gpu"];

function readEngineWindow(
  ref: RefObject<HTMLIFrameElement | null> | null,
): { progress: number | null; stage: BootStage | null; ready: boolean; crashed: boolean } {
  try {
    const w = ref?.current?.contentWindow as
      | (Window & {
          dclLoadingProgress?: unknown;
          dclLoadingStep?: unknown;
          dclEngineReady?: unknown;
          __engineCrashed?: unknown;
        })
      | null
      | undefined;
    if (!w) return { progress: null, stage: null, ready: false, crashed: false };
    const p = w.dclLoadingProgress;
    const s = w.dclLoadingStep;
    return {
      progress: typeof p === "number" && isFinite(p) ? p : null,
      stage: BOOT_STAGES.includes(s as BootStage) ? (s as BootStage) : null,
      ready: w.dclEngineReady === true,
      crashed: Boolean(w.__engineCrashed),
    };
  } catch {
    return { progress: null, stage: null, ready: false, crashed: false };
  }
}

function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}m ${String(s).padStart(2, "0")}s`;
}

export type EditorEngineStatus = "connecting" | "online" | "offline";

export interface DclEditorChromeProps {
  children?: ReactNode;
  viewportSrc?: string | null;
  viewportRef?: RefObject<HTMLIFrameElement | null> | null;
  sceneReady?: boolean;
  loading?: boolean;
  loadError?: boolean;
  onEngineStatus?: ((status: EditorEngineStatus) => void) | null;
}

export default function DclEditorChrome({
  children,
  viewportSrc = null,
  viewportRef = null,
  sceneReady = undefined,
  loading = false,
  loadError = false,
  onEngineStatus = null,
}: DclEditorChromeProps) {
  const [bootNonce, setBootNonce] = useState(0);
  const [boot, dispatchBoot] = useReducer(bootReducer, INITIAL_BOOT);
  const [engineLost, setEngineLost] = useState(false);
  const [prepTimedOut, setPrepTimedOut] = useState(false);
  const lostTicksRef = useRef(0);

  useEffect(() => {
    dispatchBoot({ type: "viewport", src: viewportSrc });
    lostTicksRef.current = 0;
    setEngineLost(false);
  }, [viewportSrc, bootNonce]);

  useEffect(() => {
    if (sceneReady) dispatchBoot({ type: "scene-ready" });
  }, [sceneReady]);

  useEffect(() => {
    if (!viewportSrc) return undefined;
    const id = setInterval(() => {
      const { progress, stage, ready, crashed } = readEngineWindow(viewportRef);
      if (boot.phase !== "ready") {
        if (progress != null) dispatchBoot({ type: "progress", pct: progress, stage });
        if (ready) dispatchBoot({ type: "engine-ready" });
      }
      if (crashed) {
        lostTicksRef.current = 0;
        setEngineLost(true);
      } else if (ready) {
        lostTicksRef.current = 0;
        setEngineLost(false);
      } else if (boot.phase === "ready") {
        lostTicksRef.current += 1;
        if (lostTicksRef.current >= 2) setEngineLost(true);
      }
    }, BOOT_PROGRESS_POLL_MS);
    return () => clearInterval(id);
  }, [viewportSrc, viewportRef, boot.phase, bootNonce]);

  useEffect(() => {
    if (!viewportSrc || boot.phase === "ready" || boot.phase === "error") return undefined;
    const id = setTimeout(() => dispatchBoot({ type: "timeout" }), BOOT_TIMEOUT_MS);
    return () => clearTimeout(id);
  }, [viewportSrc, boot.phase, bootNonce]);

  useEffect(() => {
    if (viewportSrc || !loading || loadError) {
      setPrepTimedOut(false);
      return undefined;
    }
    const id = setTimeout(() => setPrepTimedOut(true), BOOT_TIMEOUT_MS);
    return () => clearTimeout(id);
  }, [viewportSrc, loading, loadError]);

  const prepFailed = !viewportSrc && (loadError || prepTimedOut);
  const status: EditorEngineStatus =
    boot.phase === "error" || engineLost || prepFailed
      ? "offline"
      : boot.phase === "ready"
        ? "online"
        : "connecting";
  const onEngineStatusRef = useRef(onEngineStatus);
  onEngineStatusRef.current = onEngineStatus;
  useEffect(() => {
    onEngineStatusRef.current?.(status);
  }, [status]);

  const retryBoot = () => {
    dispatchBoot({ type: "retry" });
    setBootNonce((n) => n + 1);
  };

  const overlay = prepFailed
    ? ({
        show: true,
        kind: "error",
        text: loadError
          ? "The editor couldn't prepare your project files."
          : "Preparing your project is taking longer than usual.",
      } as const)
    : loading && !viewportSrc
      ? ({ show: true, kind: "loading", text: "Preparing project files\u{2026}" } as const)
      : bootOverlay(boot);
  const overlayLoading = overlay.show && overlay.kind === "loading";

  // The curtain has to outlive its own condition. It used to unmount on the frame
  // the engine reported ready, so the busiest visual moment in the editor -- a
  // full-bleed blurred overlay over a canvas that has just appeared -- ended with
  // a hard cut. Keeping it mounted for one animation lets .is-leaving fade it out
  // over the first frame instead. wasLoading, not a plain !overlayLoading, so the
  // very first render does not play a leave animation for a curtain that was
  // never shown.
  const wasLoading = useRef(false);
  const [bootLeaving, setBootLeaving] = useState(false);
  useEffect(() => {
    if (overlayLoading) {
      wasLoading.current = true;
      setBootLeaving(false);
      return undefined;
    }
    if (!wasLoading.current) return undefined;
    wasLoading.current = false;
    setBootLeaving(true);
    const t = setTimeout(() => setBootLeaving(false), BOOT_LEAVE_MS);
    return () => clearTimeout(t);
  }, [overlayLoading]);

  const [elapsedS, setElapsedS] = useState(0);
  useEffect(() => {
    if (!overlayLoading) {
      setElapsedS(0);
      return undefined;
    }
    const started = Date.now();
    const id = setInterval(() => setElapsedS(Math.round((Date.now() - started) / 1000)), 1000);
    return () => clearInterval(id);
  }, [overlayLoading]);
  const retry = prepFailed
    ? () => {
        if (typeof window !== "undefined") window.location.reload();
      }
    : retryBoot;

  return (
    <div className="eui-root eui-viewport ui2" role="region" aria-label="DCL Editor">
      {viewportSrc ? (
        <EngineViewport
          key={bootNonce}
          viewportRef={viewportRef}
          src={viewportSrc}
          onLoad={() => dispatchBoot({ type: "iframe-load" })}
        />
      ) : (
        <>
          <div className="eui-vp-horizon" aria-hidden="true" />
          <div className="eui-vp-grid" aria-hidden="true" />
          <div className="eui-vp-object" aria-hidden="true">
            <div className="eui-vp-cube" />
            <div className="eui-vp-gizmo">
              <i className="ax-z" />
              <i className="ax-x" />
              <i className="ax-y" />
            </div>
          </div>
        </>
      )}
      {overlayLoading || bootLeaving ? (
        <div
          className={"eui-boot" + (bootLeaving ? " is-leaving" : "")}
          role={bootLeaving ? undefined : "status"}
          aria-hidden={bootLeaving ? true : undefined}
        >
          <span className="eui-boot-spinner" aria-hidden="true" />
          <span className="eui-boot-stage">{overlay.text}</span>
          {elapsedS >= 1 ? (
            <span className="eui-boot-elapsed">{formatElapsed(elapsedS)}</span>
          ) : null}
        </div>
      ) : null}
      {overlay.show && overlay.kind === "error" ? (
        <div className="eui-boot eui-boot--error" role="alert">
          <span>{overlay.text} This sometimes happens on first load.</span>
          <button className="eui-btn primary" type="button" onClick={retry}>
            Retry
          </button>
        </div>
      ) : null}
      {children}
      <MobileEditorGate
        title="Open the scene editor on a desktop"
        message="The scene editor needs a wider screen and a WebGPU-capable desktop browser to run the 3D engine. Come back on a laptop or desktop to keep building."
        backHref="/create/scenes"
        backLabel="Back to your scenes"
      />
    </div>
  );
}
