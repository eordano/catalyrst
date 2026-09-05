import type { RefObject } from "react";
import { useEffect, useState } from "react";
import type { EditorTool } from "../bus-protocol";
import type { EditorBus } from "../editor-bus";
import { createEditorBus } from "../editor-bus";
import { INIT_ANNOUNCE_INTERVAL_MS } from "../editor-config";
import { cloneValue, type HistoryEngine, type HistoryEntry } from "../history";
import { buildLiveTree } from "../live-tree";
import { quantizeTransform, type SnapState } from "../snap";
import { resolveCompositeAssets } from "../project-cache";
import type { CameraPrefs, DeTreeNode, EditorTransform, EditorVec } from "../types";
import type { LiveSceneInfo } from "../../generated/editor-bus";

const hydratedComposites = new Set<string>();

export interface LiveSelection {
  selected: string[];
  active: string | null;
}

interface EditorBusBridgeOptions {
  live: boolean;
  title: string;
  viewportRef: RefObject<HTMLIFrameElement | null>;
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

export interface EditorCameraPose {
  x: number;
  y: number;
  z: number;
  yaw: number;
  pitch: number;
}

export function useEditorBusBridge({
  live,
  title,
  viewportRef,
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
  const [sceneReady, setSceneReady] = useState(false);
  const [liveSel, setLiveSel] = useState<LiveSelection | null>(null);
  const [liveComps, setLiveComps] = useState<Record<string, string[]>>({});
  const [liveXform, setLiveXform] = useState<Record<string, EditorTransform>>({});
  const [cameraPose, setCameraPose] = useState<EditorCameraPose | null>(null);
  const [liveTree, setLiveTree] = useState<DeTreeNode[] | null>(null);
  const [liveScene, setLiveScene] = useState<LiveSceneInfo | null>(null);
  const [orientGlobal, setOrientGlobal] = useState(false);

  useEffect(() => {
    if (!live) return undefined;
    const bus = createEditorBus();
    if (!bus.ok) return undefined;
    busRef.current = bus;
    let handshook = false;
    let initTimer: ReturnType<typeof setInterval> | null = null;
    const stopInit = () => {
      if (initTimer != null) {
        clearInterval(initTimer);
        initTimer = null;
      }
    };
    const off = bus.onMessage((msg) => {
      if (!msg || typeof msg !== "object") return;
      switch (msg.type) {
        case "scene-ready":
          handshook = true;
          stopInit();
          setSceneReady(true);
          setLiveScene(msg.scene ?? null);
          setOrientGlobal(msg.orientGlobal === true);
          setLiveSel({ selected: msg.selected ?? [], active: msg.active ?? null });
          if (msg.tool) setTool(msg.tool);
          busRef.current?.setCameraSettings(prefsRef.current);
          {
            const rc = rawCompositeRef.current;
            if (rc && !hydratedComposites.has(rc)) {
              hydratedComposites.add(rc);
              busRef.current?.loadScene(resolveCompositeAssets(rc));
            }
          }
          break;
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
            // The drag itself stays free; the value lands on the grid on
            // release, and the engine is told the rounded value so the render
            // and the inspector cannot disagree.
            const batch: HistoryEntry[] = [];
            const moved: Record<string, EditorTransform> = {};
            for (const [eid, t] of Object.entries(raw)) {
              const prev = compValuesRef.current[eid]?.Transform as
                | (EditorTransform & { parent?: number })
                | undefined;
              const after = snap.on ? quantizeTransform(t, snap, prev) : t;
              const before = cloneValue(prev);
              // A Transform written without its parent reparents the entity to
              // the scene root, so the echo carries the parent forward.
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
    initTimer = setInterval(() => {
      if (handshook) return stopInit();
      bus.init();
    }, INIT_ANNOUNCE_INTERVAL_MS);
    return () => {
      stopInit();
      off();
      bus.close();
      busRef.current = null;
      setSceneReady(false);
      setLiveScene(null);
      setLiveSel(null);
      setLiveComps({});
      setLiveTree(null);
      setCameraPose(null);
    };
  }, [live]);

  useEffect(() => {
    if (!live) return undefined;
    const f = viewportRef.current;
    if (!f || typeof f.addEventListener !== "function") return undefined;
    const onLoad = () => {
      const rc = rawCompositeRef.current;
      if (rc) hydratedComposites.delete(rc);
    };
    f.addEventListener("load", onLoad);
    return () => f.removeEventListener("load", onLoad);
  }, [live]);

  return {
    sceneReady,
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
