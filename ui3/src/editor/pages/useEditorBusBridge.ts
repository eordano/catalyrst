import type { RefObject } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { EditorTool } from "../bus-protocol";
import type { EditorBus } from "../editor-bus";
import { createEditorBus } from "../editor-bus";
import { BOOT_TIMEOUT_MS, INIT_ANNOUNCE_INTERVAL_MS, RESTORE_COMPOSITE_TIMEOUT_MS } from "../editor-config";
import { cloneValue, type HistoryEngine, type HistoryEntry } from "../history";
import { buildLiveTree } from "../live-tree";
import { quantizeTransform, type SnapState } from "../snap";
import { resolveCompositeAssets } from "../project-cache";
import { canRestoreSceneSession, createSceneSession } from "./scene-session";
import type { CameraPrefs, DeTreeNode, EditorTransform, EditorVec } from "../types";
import type { LiveSceneInfo } from "../../generated/editor-bus";

interface LiveSelection {
  selected: string[];
  active: string | null;
}

interface EditorBusBridgeOptions {
  prepareScene?: () => Promise<void>;
  live: boolean;
  viewportSrc: string | null | undefined;
  title: string;
  busRef: RefObject<EditorBus | null>;
  prefsRef: RefObject<CameraPrefs>;
  rawCompositeRef: RefObject<string | null>;
  compValuesRef: RefObject<Record<string, Record<string, unknown>>>;
  historyRef: RefObject<HistoryEngine | null>;
  activeIdRef: RefObject<string | null>;
  nudgeBaseRef: RefObject<{
    id: string;
    position: EditorVec;
    euler: EditorVec;
    scale: EditorVec;
  } | null>;
  setTool: (tool: EditorTool) => void;
  notePlayEdit: () => void;
  snapRef: RefObject<SnapState>;
}

interface EditorCameraPose {
  x: number;
  y: number;
  z: number;
  yaw: number;
  pitch: number;
}

export function useEditorBusBridge({
  prepareScene,
  live,
  viewportSrc,
  title,
  busRef,
  prefsRef,
  rawCompositeRef,
  compValuesRef,
  historyRef,
  activeIdRef,
  nudgeBaseRef,
  setTool,
  notePlayEdit,
  snapRef,
}: EditorBusBridgeOptions) {
  const prepareSceneRef = useRef(prepareScene);
  prepareSceneRef.current = prepareScene;
  const [sceneReady, setSceneReady] = useState(false);
  const [session, setSession] = useState(0);
  const [sceneError, setSceneError] = useState<string | null>(null);
  const onViewportLoad = useCallback(() => {
    setSceneReady(false);
    setSession((value) => value + 1);
  }, []);
  const [liveSel, setLiveSel] = useState<LiveSelection | null>(null);
  const [liveComps, setLiveComps] = useState<Record<string, string[]>>({});
  const [liveXform, setLiveXform] = useState<Record<string, EditorTransform>>({});
  const [cameraPose, setCameraPose] = useState<EditorCameraPose | null>(null);
  const [liveTree, setLiveTree] = useState<DeTreeNode[] | null>(null);
  const [liveScene, setLiveScene] = useState<LiveSceneInfo | null>(null);
  const [orientGlobal, setOrientGlobal] = useState(false);

  useEffect(() => {
    if (!live) return undefined;
    const bus = createEditorBus(viewportSrc);
    if (!bus.ok) return undefined;
    busRef.current = bus;
    const lifecycle = createSceneSession(session);
    setSceneReady(false);
    setSceneError(null);
    compValuesRef.current = {};
    historyRef.current?.clear();
    nudgeBaseRef.current = null;
    let initTimer: ReturnType<typeof setInterval> | null = null;
    let handshakeTimer: ReturnType<typeof setTimeout> | null = null;
    let hydrationTimer: ReturnType<typeof setTimeout> | null = null;
    const stopInit = () => {
      if (initTimer != null) {
        clearInterval(initTimer);
        initTimer = null;
      }
    };
    const stopHandshake = () => {
      if (handshakeTimer !== null) {
        clearTimeout(handshakeTimer);
        handshakeTimer = null;
      }
    };
    const stopHydration = () => {
      if (hydrationTimer !== null) {
        clearTimeout(hydrationTimer);
        hydrationTimer = null;
      }
    };
    const off = bus.onMessage((msg) => {
      if (!msg || typeof msg !== "object") return;
      switch (msg.type) {
        case "scene-ready": {
          if (!msg.scene) break;
          if (lifecycle.phase() !== "handshake") break;
          setLiveScene(msg.scene ?? null);
          setOrientGlobal(msg.orientGlobal === true);
          setLiveSel({ selected: msg.selected ?? [], active: msg.active ?? null });
          if (msg.tool) setTool(msg.tool);
          const rc = rawCompositeRef.current;
          if (rc || prepareSceneRef.current) {
            const token = lifecycle.beginHydration();
            if (token === null) break;
            stopInit();
            stopHandshake();
            bus.setCameraSettings(prefsRef.current);
            hydrationTimer = setTimeout(() => {
              if (lifecycle.timeoutHydration(token)) {
                setSceneError("Your scene could not be loaded into the editor. Retry to reconnect and load it again.");
              }
            }, RESTORE_COMPOSITE_TIMEOUT_MS);
            const prepare = prepareSceneRef.current;
            const restore = () => {
              if (!canRestoreSceneSession(lifecycle, token, busRef.current === bus)) return Promise.resolve();
              return rc
                ? bus.rpc("restoreComposite", [resolveCompositeAssets(rc)], RESTORE_COMPOSITE_TIMEOUT_MS)
                : Promise.resolve();
            };
            const prepared = Promise.resolve().then(() => canRestoreSceneSession(lifecycle, token, busRef.current === bus) ? prepare?.() : undefined).then(restore);
            void prepared.then(() => {
              stopHydration();
              if (busRef.current === bus && lifecycle.complete(token)) setSceneReady(true);
            }).catch(() => {
              stopHydration();
              if (lifecycle.fail(token)) setSceneError("Your scene could not be loaded into the editor. Retry to reconnect and load it again.");
            });
          } else if (lifecycle.readyWithoutHydration()) {
            stopInit();
            stopHandshake();
            bus.setCameraSettings(prefsRef.current);
            setSceneReady(true);
          }
          break;
        }
        case "selection": {
          setLiveSel({ selected: msg.selected ?? [], active: msg.active ?? null });
          const comps = (msg as { components?: Record<string, Record<string, unknown>> })
            .components;
          if (comps && typeof comps === "object") {
            setLiveComps((prev) => {
              const next = { ...prev };
              for (const [eid, byName] of Object.entries(comps)) {
                if (byName && typeof byName === "object") {
                  next[eid] = Object.keys(byName);
                }
              }
              return next;
            });
            for (const [eid, byName] of Object.entries(comps)) {
              if (byName && typeof byName === "object") {
                compValuesRef.current[eid] = byName as Record<string, unknown>;
              }
            }
            setLiveXform((prev) => {
              let next = prev;
              for (const [eid, byName] of Object.entries(comps)) {
                const t = (byName as Record<string, unknown> | null)?.Transform as
                  | EditorTransform
                  | undefined;
                if (t && typeof t === "object") {
                  if (next === prev) next = { ...prev };
                  next[eid] = t;
                  if (nudgeBaseRef.current?.id === eid) nudgeBaseRef.current = null;
                }
              }
              return next;
            });
          }
          break;
        }
        case "entities":
          setLiveTree(buildLiveTree(msg.entities ?? [], title));
          break;
        case "camera-pose":
          setCameraPose({ x: msg.x, y: msg.y, z: msg.z, yaw: msg.yaw, pitch: msg.pitch });
          break;
        case "tool":
          if (msg.tool) setTool(msg.tool);
          break;
        case "drag-end":
          if (msg.transforms && typeof msg.transforms === "object") {
            const snap = snapRef.current;
            const raw = msg.transforms as Record<string, EditorTransform>;
            const batch: HistoryEntry[] = [];
            const moved: Record<string, EditorTransform> = {};
            for (const [eid, t] of Object.entries(raw)) {
              const prev = compValuesRef.current[eid]?.Transform as
                | (EditorTransform & { parent?: number })
                | undefined;
              const after = snap.on ? quantizeTransform(t, snap, prev) : t;
              const before = cloneValue(prev);
              const echo: EditorTransform & { parent?: number } = { ...after };
              if (prev && typeof prev.parent === "number") echo.parent = prev.parent;
              if (before !== undefined) {
                batch.push({ entity: eid, name: "Transform", before, after: cloneValue(echo) });
              }
              (compValuesRef.current[eid] ??= {}).Transform = cloneValue(echo);
              moved[eid] = after;
              if (snap.on) busRef.current?.setComponent(eid, "Transform", JSON.stringify(echo));
            }
            if (batch.length > 0) historyRef.current?.push(batch);
            setLiveXform((prev) => ({ ...prev, ...moved }));
            if (activeIdRef.current != null && activeIdRef.current in moved) {
              nudgeBaseRef.current = null;
            }
            notePlayEdit();
          }
          break;
        default:
          break;
      }
    });
    bus.init();
    handshakeTimer = setTimeout(() => {
      if (lifecycle.timeoutHandshake()) {
        stopInit();
        setSceneError("The editor did not respond. Retry to reconnect and load your scene again.");
      }
    }, BOOT_TIMEOUT_MS);
    initTimer = setInterval(() => {
      if (lifecycle.phase() !== "handshake") return stopInit();
      bus.init();
    }, INIT_ANNOUNCE_INTERVAL_MS);
    return () => {
      lifecycle.retire();
      stopInit();
      stopHandshake();
      stopHydration();
      off();
      bus.close();
      if (busRef.current === bus) busRef.current = null;
      setSceneReady(false);
      setLiveScene(null);
      setLiveSel(null);
      setLiveComps({});
      setLiveXform({});
      setLiveTree(null);
      setCameraPose(null);
    };
  }, [live, viewportSrc, session]);

  return {
    sceneReady,
    sceneError,
    onViewportLoad,
    session,
    liveSel,
    setLiveSel,
    liveComps,
    setLiveComps,
    liveXform,
    setLiveXform,
    liveTree,
    liveScene,
    cameraPose,
    orientGlobal,
    setOrientGlobal,
  };
}
