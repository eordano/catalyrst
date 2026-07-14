import type { RefObject } from "react";
import { useEffect, useRef, useState } from "react";
import type { EditorBridgeAction } from "../../overlay/editor-bridge-types";
import type { EditorBus } from "../editor-bus";
import {
  createStepDebugger,
  engineConsoleRunner,
  type DebugSystemsView,
  type EntityDiff,
  type StepDebugger,
} from "../debugger";
import { setProjectPlayState } from "../project-cache";

interface DebugUiState {
  tick: number | null;
  stepping: boolean;
  error: string | null;
  entries: EntityDiff[] | null;
  lastStepCount: number | null;
  unchanged: number;
  total: number;
  timedOut: boolean;
  systems: DebugSystemsView | null;
}

const EMPTY_DEBUG_UI: DebugUiState = {
  tick: null,
  stepping: false,
  error: null,
  entries: null,
  lastStepCount: null,
  unchanged: 0,
  total: 0,
  timedOut: false,
  systems: null,
};

export const DEBUG_RESERVED_LABELS: Record<string, string> = {
  "0": "Scene Root",
  "1": "Player",
  "2": "Camera",
};

interface DebugSessionOptions {
  viewportRef: RefObject<HTMLIFrameElement | null>;
  busRef: RefObject<EditorBus | null>;
  playStateRef: RefObject<{ playing: boolean; paused: boolean }>;
  postToViewport: (action: EditorBridgeAction, extra?: { count?: number }) => void;
  setRunPaused: (paused: boolean) => void;
}

export function useDebugSession({
  viewportRef,
  busRef,
  playStateRef,
  postToViewport,
  setRunPaused,
}: DebugSessionOptions) {
  const [debugOpen, setDebugOpen] = useState(false);
  const [debugHeight, setDebugHeight] = useState(280);
  const [debugUi, setDebugUi] = useState<DebugUiState>(EMPTY_DEBUG_UI);
  const debugCtlRef = useRef<StepDebugger | null>(null);
  const debugOpenRef = useRef(false);
  debugOpenRef.current = debugOpen;

  const exitDebug = (clearFlag = true) => {
    debugCtlRef.current?.dispose();
    debugCtlRef.current = null;
    setDebugOpen(false);
    if (clearFlag) void setProjectPlayState(true, false);
  };

  const enterDebug = () => {
    if (!playStateRef.current.playing || debugOpenRef.current) return;
    void setProjectPlayState(true, true);
    if (!playStateRef.current.paused) {
      postToViewport("FreezeScene");
      setRunPaused(true);
      busRef.current?.announcePlayState(true, true);
    }
    setDebugUi(EMPTY_DEBUG_UI);
    setDebugOpen(true);
    const run = engineConsoleRunner(viewportRef.current);
    if (!run) {
      setDebugUi((prev) => ({
        ...prev,
        error: "Engine console unavailable \u{2014} the engine may still be starting.",
      }));
      return;
    }
    const ctl = createStepDebugger({ run });
    debugCtlRef.current = ctl;
    ctl
      .open()
      .then((res) => {
        if (debugCtlRef.current !== ctl) return;
        setDebugUi((prev) => ({
          ...prev,
          tick: res.tick,
          total: res.entities,
          systems: res.systems ?? prev.systems,
        }));
      })
      .catch((e) => {
        if (debugCtlRef.current !== ctl) return;
        setDebugUi((prev) => ({ ...prev, error: "Snapshot failed: " + String(e) }));
      });
  };

  const debugStep = (count: number) => {
    const ctl = debugCtlRef.current;
    if (!ctl || ctl.isBusy()) return;
    setDebugUi((prev) => ({ ...prev, stepping: true, error: null }));
    ctl
      .step(count)
      .then((res) => {
        if (debugCtlRef.current !== ctl) return;
        if (!res) {
          setDebugUi((prev) => ({ ...prev, stepping: false }));
          return;
        }
        setDebugUi((prev) => ({
          tick: res.tick,
          stepping: false,
          error: null,
          entries: res.entries,
          lastStepCount: res.count,
          unchanged: res.unchangedEntities,
          total: res.totalEntities,
          timedOut: res.timedOut,
          systems: res.systems ?? prev.systems,
        }));
      })
      .catch((e) => {
        if (debugCtlRef.current !== ctl) return;
        setDebugUi((prev) => ({ ...prev, stepping: false, error: "Step failed: " + String(e) }));
      });
  };

  useEffect(
    () => () => {
      debugCtlRef.current?.dispose();
      debugCtlRef.current = null;
    },
    [],
  );

  return {
    debugOpen,
    debugOpenRef,
    debugHeight,
    setDebugHeight,
    debugUi,
    enterDebug,
    exitDebug,
    debugStep,
  };
}
