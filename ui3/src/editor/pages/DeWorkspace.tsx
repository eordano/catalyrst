import type { RefObject } from "react";
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { EditorBridgeAction, EditorBridgeRequest } from "../../overlay/editor-bridge-types";
import type { EditorTool } from "../bus-protocol";
import type { EditorBus, EditorCamMode } from "../editor-bus";
import type {
  AuthorComponentFn,
  CameraPrefs,
  DeCatalogItem,
  DeInspector,
  DeleteComponentFn,
  DeLocalItem,
  DeTreeNode,
  DeWorkspaceCode,
  EditorTransform,
  EditorVec,
} from "../types";
import DclEditorChrome, { type EditorEngineStatus } from "../frames/DclEditorChrome";
import { createHistory, cloneValue, type HistoryEngine } from "../history";
import { forwardEngineKeys } from "../shortcuts";
import { quatToEulerDeg, eulerDegToQuat, isQuat, tidy } from "../transform-nudge";
import { attachCameraInput } from "../camera-input";
import { loadCameraPrefs, saveCameraPrefs } from "../camera-prefs";
import { placeAssetOnBus, setProjectPlayState } from "../project-cache";
import { findNodeName } from "../live-tree";
import DeCameraSettings from "../components/DeCameraSettings";
import DeDebugPanel from "../components/DeDebugPanel";
import DeShortcutsOverlay from "../components/DeShortcutsOverlay";
import { DeAssetsPanel } from "../components/DeAssetsPanel";
import { DeHierarchyPanel } from "../components/DeHierarchyPanel";
import {
  DeInspectorPanel,
  DUPLICATE_SKIP,
  isTransformComp,
  type NudgeFieldFn,
} from "../components/DeInspectorPanel";
import { DeToolbar, type DeToolbarProps } from "../components/DeToolbar";
import { PlayEditWarningModal } from "../components/DePlayEditWarning";
import { useDebugSession, DEBUG_RESERVED_LABELS } from "./useDebugSession";
import { useEditorBusBridge } from "./useEditorBusBridge";
import { useProjectRealm } from "./useProjectRealm";
import { useWorkspaceShortcuts } from "./useWorkspaceShortcuts";

export {
  IconSelect,
  IconMove,
  IconRotate,
  IconScale,
  IconPlay,
  IconPause,
  IconStep,
  IconStop,
  IconBug,
  IconDots,
  IconPlus,
  IconBolt,
  IconImport,
  IconTrash,
  IconSidebarLeft,
  IconSidebarRight,
  IconCamera,
  IconEdit,
  IconUndo,
  IconRedo,
  ModelGlyph,
} from "../components/DeIcons";
export { DeToolbar } from "../components/DeToolbar";
export type { DeToolbarProps } from "../components/DeToolbar";
export { DeContextMenu, DeHierarchyPanel, DeLeftTabs } from "../components/DeHierarchyPanel";
export type { DeContextMenuProps, DeHierarchyPanelProps } from "../components/DeHierarchyPanel";
export { DeAddComponentPicker, DeInspectorPanel } from "../components/DeInspectorPanel";
export type { DeInspectorPanelProps } from "../components/DeInspectorPanel";
export { DeAssetsPanel, DeCatalogTab, DeLocalTab } from "../components/DeAssetsPanel";
export type {
  DeAssetsPanelProps,
  DeCatalogTabProps,
  DeLocalTabProps,
} from "../components/DeAssetsPanel";

const DeCodeWorkspace = lazy(() => import("../code/DeCodeWorkspace"));

const PLAY_EDIT_WARNED_KEY = "dcl-editor:play-edit-warned";

export interface DeWorkspaceProps {
  left?: "scene" | "assets";
  title?: string;
  tree?: DeTreeNode[];
  inspector?: DeInspector;
  addOpen?: boolean;
  catalog?: DeCatalogItem[];
  local?: DeLocalItem[];
  viewportSrc?: string | null;
  rawComposite?: string | null;
  code?: DeWorkspaceCode | null;
  prepareRealm?: (() => Promise<unknown>) | null;
  onEngineStatus?: ((status: EditorEngineStatus) => void) | null;
}

export default function DeWorkspace({
  left = "scene",
  title = "",
  tree = [],
  inspector = {},
  addOpen = false,
  catalog = [],
  local = [],
  viewportSrc = null,
  rawComposite = null,
  code = null,
  prepareRealm = null,
  onEngineStatus = null,
}: DeWorkspaceProps) {
  const viewportRef = useRef<HTMLIFrameElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [runPaused, setRunPaused] = useState(false);
  const playStateRef = useRef({ playing: false, paused: false });
  playStateRef.current = { playing, paused: runPaused };
  const [playEditWarn, setPlayEditWarn] = useState(false);
  const playEditNotedRef = useRef(false);
  const notePlayEdit = () => {
    if (!playStateRef.current.playing || playEditNotedRef.current) return;
    playEditNotedRef.current = true;
    try {
      if (window.localStorage?.getItem(PLAY_EDIT_WARNED_KEY) === "1") return;
    } catch {
    }
    setPlayEditWarn(true);
  };
  const dismissPlayEditWarn = (dontShowAgain: boolean) => {
    setPlayEditWarn(false);
    if (!dontShowAgain) return;
    try {
      window.localStorage?.setItem(PLAY_EDIT_WARNED_KEY, "1");
    } catch {
    }
  };
  const [codeOpen, setCodeOpen] = useState(false);
  const [tool, setTool] = useState<EditorTool>("translate");
  const [hideLeft, setHideLeft] = useState(false);
  const [hideRight, setHideRight] = useState(false);
  const codeStoreRef = useRef<Map<string, string>>(new Map());

  const realmStatus = useProjectRealm(viewportSrc, prepareRealm);
  const effViewportSrc = realmStatus === "ready" ? viewportSrc : null;

  const [engineStatus, setEngineStatus] = useState<EditorEngineStatus>("connecting");
  const onEngineStatusRef = useRef(onEngineStatus);
  onEngineStatusRef.current = onEngineStatus;
  const handleEngineStatus = useCallback((s: EditorEngineStatus) => {
    setEngineStatus(s);
    onEngineStatusRef.current?.(s);
  }, []);

  const live = !!effViewportSrc;
  const postToFrame = (
    ref: RefObject<HTMLIFrameElement | null>,
    src: string | null | undefined,
    action: EditorBridgeAction,
    extra?: { count?: number; requestId?: string | number },
  ) => {
    const f = ref.current;
    if (!f || !f.contentWindow) return;
    let target = "*";
    try {
      target = new URL(String(src)).origin;
    } catch {
    }
    const msg = { type: "dcl-bridge", action, ...(extra || {}) } as EditorBridgeRequest;
    f.contentWindow.postMessage(msg, target);
  };
  const postToViewport = (action: EditorBridgeAction, extra?: { count?: number }) =>
    postToFrame(viewportRef, effViewportSrc, action, extra);
  const prePlayRef = useRef<string | null>(null);

  const busRef = useRef<EditorBus | null>(null);

  const {
    debugOpen,
    debugOpenRef,
    debugHeight,
    setDebugHeight,
    debugUi,
    enterDebug,
    exitDebug,
    debugStep,
  } = useDebugSession({ viewportRef, busRef, playStateRef, postToViewport, setRunPaused });

  const controls: Partial<DeToolbarProps> = live
    ? {
        playing: playing && !runPaused,
        onPlay: () => {
          if (playing && runPaused) {
            if (debugOpenRef.current) exitDebug();
            postToViewport("UnfreezeScene");
            setRunPaused(false);
            busRef.current?.announcePlayState(true, false);
            return;
          }
          const bus = busRef.current;
          const begin = () => {
            void setProjectPlayState(true);
            postToViewport("UnfreezeScene");
            setRunPaused(false);
            setPlaying(true);
            playEditNotedRef.current = false;
            busRef.current?.announcePlayState(true, false);
          };
          if (bus?.exportComposite) {
            bus
              .exportComposite()
              .then((c) => {
                prePlayRef.current = typeof c === "string" && c.trim() !== "" ? c : null;
              })
              .catch(() => {
                prePlayRef.current = null;
              })
              .finally(begin);
          } else {
            prePlayRef.current = null;
            begin();
          }
        },
        onPause: () => {
          if (!playing || runPaused) return;
          postToViewport("FreezeScene");
          setRunPaused(true);
          busRef.current?.announcePlayState(true, true);
        },
        onStep: () => {
          if (debugOpenRef.current) {
            debugStep(1);
            return;
          }
          postToViewport("TickScene", { count: 1 });
        },
        onDebug: playing
          ? () => {
              if (debugOpenRef.current) exitDebug();
              else enterDebug();
            }
          : undefined,
        debugActive: debugOpen,
        onStop: playing
          ? () => {
              if (debugOpenRef.current) exitDebug(false);
              void setProjectPlayState(false);
              if (runPaused) postToViewport("UnfreezeScene");
              const pre = prePlayRef.current;
              prePlayRef.current = null;
              if (pre) busRef.current?.loadScene?.(pre, true);
              setPlaying(false);
              setRunPaused(false);
              playEditNotedRef.current = false;
              setPlayEditWarn(false);
              busRef.current?.announcePlayState(false, false);
            }
          : undefined,
      }
    : {};

  const rawCompositeRef = useRef<string | null>(rawComposite);
  rawCompositeRef.current = rawComposite;
  const [camMode, setCamMode] = useState<EditorCamMode>("target");
  const [assetsOverride, setAssetsOverride] = useState(false);
  const [camPrefs, setCamPrefs] = useState<CameraPrefs>(() => loadCameraPrefs());
  const [camSettingsOpen, setCamSettingsOpen] = useState(false);
  const prefsRef = useRef<CameraPrefs>(camPrefs);
  prefsRef.current = camPrefs;
  const camModeRef = useRef<EditorCamMode>(camMode);
  camModeRef.current = camMode;
  const activeIdRef = useRef<string | null>(null);
  const nudgeBaseRef = useRef<{
    id: string;
    position: EditorVec;
    euler: EditorVec;
    scale: EditorVec;
  } | null>(null);

  const [, setHistoryVersion] = useState(0);
  const compValuesRef = useRef<Record<string, Record<string, unknown>>>({});
  const applyHistoryWriteRef = useRef<(entity: string, name: string, value: unknown) => void>(
    () => {},
  );
  const historyRef = useRef<HistoryEngine | null>(null);
  if (historyRef.current === null) {
    historyRef.current = createHistory(
      (entity, name, value) => applyHistoryWriteRef.current(entity, name, value),
      () => setHistoryVersion((v) => v + 1),
    );
  }
  const history = historyRef.current;

  const {
    sceneReady,
    liveSel,
    setLiveSel,
    liveComps,
    setLiveComps,
    liveXform,
    setLiveXform,
    liveTree,
  } = useEditorBusBridge({
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
  });
  activeIdRef.current = liveSel?.active ?? null;

  applyHistoryWriteRef.current = (entity, name, value) => {
    notePlayEdit();
    const key = String(entity);
    if (value === undefined) {
      busRef.current?.deleteComponent(entity, name);
      const vals = compValuesRef.current[key];
      if (vals) delete vals[name];
      setLiveComps((prev) => {
        const cur = prev[key];
        return cur ? { ...prev, [key]: cur.filter((c) => c !== name) } : prev;
      });
      return;
    }
    busRef.current?.setComponent(entity, name, JSON.stringify(value));
    (compValuesRef.current[key] ??= {})[name] = cloneValue(value);
    setLiveComps((prev) => {
      const cur = prev[key] ?? [];
      return cur.includes(name) ? prev : { ...prev, [key]: [...cur, name] };
    });
    if (isTransformComp(name)) {
      setLiveXform((prev) => ({ ...prev, [key]: value as EditorTransform }));
    }
  };

  const onPrefsChange = (next: CameraPrefs) => {
    const norm = saveCameraPrefs(next);
    setCamPrefs(norm);
    busRef.current?.setCameraSettings(norm);
  };

  useEffect(() => {
    if (typeof window === "undefined") return undefined;
    const optedIn =
      /[?&]mcp=/.test(window.location.search) || !!window.localStorage?.getItem("dcl-mcp-relay");
    if (!optedIn) return undefined;
    let gone = false;
    let dispose: (() => void) | null = null;
    import("../mcp-bridge")
      .then((m) => {
        if (gone) return;
        dispose = m.autoConnect({ getViewportEl: () => viewportRef.current });
      })
      .catch(() => {});
    return () => {
      gone = true;
      dispose?.();
    };
  }, []);

  useEffect(() => {
    if (!live || !sceneReady || typeof document === "undefined") return undefined;
    let suspended = false;
    const onVis = () => {
      const ps = playStateRef.current;
      const pausedByUser = ps.playing && ps.paused;
      if (document.visibilityState === "hidden") {
        if (!pausedByUser && !suspended) {
          postToViewport("FreezeScene");
          suspended = true;
        }
      } else if (suspended) {
        suspended = false;
        const now = playStateRef.current;
        if (!(now.playing && now.paused)) postToViewport("UnfreezeScene");
      }
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      document.removeEventListener("visibilitychange", onVis);
      if (suspended) postToViewport("UnfreezeScene");
    };
  }, [live, sceneReady]);

  useEffect(() => {
    if (!live || !sceneReady) return undefined;
    const cw = viewportRef.current?.contentWindow;
    const bus = busRef.current;
    if (!cw || !bus) return undefined;
    const detach = attachCameraInput(
      cw,
      bus,
      () => prefsRef.current,
      () => ({ camMode: camModeRef.current, activeId: activeIdRef.current }),
    );
    forwardEngineKeys(cw);
    bus.setCamMode(camModeRef.current);
    return detach;
  }, [live, sceneReady]);

  const handleTool = (t: EditorTool) => {
    setTool(t);
    busRef.current?.setTool(t);
  };

  const authorComponent: AuthorComponentFn = (entity, name, json) => {
    notePlayEdit();
    busRef.current?.setComponent(entity, name, json);
    if (entity != null) {
      try {
        const after = JSON.parse(json) as unknown;
        const key = String(entity);
        const before = cloneValue(compValuesRef.current[key]?.[name]);
        historyRef.current?.push([{ entity: key, name, before, after: cloneValue(after) }]);
        (compValuesRef.current[key] ??= {})[name] = after;
      } catch {
      }
    }
    const key = String(entity);
    setLiveComps((prev) => {
      const cur = prev[key] ?? [];
      return cur.includes(name) ? prev : { ...prev, [key]: [...cur, name] };
    });
  };

  const placeAsset = (asset: DeCatalogItem) => {
    notePlayEdit();
    return placeAssetOnBus(busRef, asset);
  };

  const busLive = live && sceneReady;
  const activeId = liveSel?.active ?? null;
  const onHierSelect = busLive
    ? (id: string | number) => busRef.current?.setSelection([String(id)], String(id))
    : undefined;

  const handleCamMode = busLive
    ? (m: EditorCamMode) => {
        setCamMode(m);
        busRef.current?.setCamMode(m);
      }
    : undefined;
  const deleteComponent: DeleteComponentFn | undefined = busLive
    ? (entity, name) => {
        notePlayEdit();
        const key = String(entity);
        const before = cloneValue(compValuesRef.current[key]?.[name]);
        if (before !== undefined) {
          historyRef.current?.push([{ entity: key, name, before, after: undefined }]);
          const vals = compValuesRef.current[key];
          if (vals) delete vals[name];
        }
        busRef.current?.deleteComponent(entity, name);
        setLiveComps((prev) =>
          prev[key] ? { ...prev, [key]: prev[key].filter((c) => c !== name) } : prev,
        );
      }
    : undefined;
  const addRootEntity = busLive
    ? () => {
        notePlayEdit();
        busRef.current?.addEntity("Entity", 0);
      }
    : undefined;
  const undo = busLive && history.canUndo() ? () => history.undo() : undefined;
  const redo = busLive && history.canRedo() ? () => history.redo() : undefined;
  const effLeft = assetsOverride ? "assets" : left;

  const effTree = live && liveTree != null ? liveTree : tree;

  const selectedIds: string[] = liveSel?.selected?.length
    ? liveSel.selected
    : activeId != null
      ? [String(activeId)]
      : [];
  const deleteSelected =
    busLive && selectedIds.length > 0
      ? () => {
          notePlayEdit();
          for (const id of selectedIds) {
            busRef.current?.deleteEntity(id, true);
            delete compValuesRef.current[String(id)];
          }
          busRef.current?.setSelection([], null);
          setLiveSel({ selected: [], active: null });
        }
      : undefined;
  const duplicateSelected =
    busLive && activeId != null
      ? () => {
          const src = String(activeId);
          const comps = compValuesRef.current[src];
          if (!comps) return;
          notePlayEdit();
          const copy: Record<string, unknown> = {};
          for (const [cname, value] of Object.entries(comps)) {
            if (DUPLICATE_SKIP.has(cname)) continue;
            copy[cname] = cloneValue(value);
          }
          const baseName = findNodeName(effTree, src) ?? `Entity ${src}`;
          const t = comps.Transform as { parent?: number } | undefined;
          busRef.current?.addEntity(`${baseName} copy`, Number(t?.parent ?? 0) || 0, copy);
        }
      : undefined;
  const clearSelection =
    busLive && (selectedIds.length > 0 || activeId != null)
      ? () => {
          busRef.current?.setSelection([], null);
          setLiveSel({ selected: [], active: null });
        }
      : undefined;

  const { shortcutsOpen, setShortcutsOpen } = useWorkspaceShortcuts({
    playing,
    camMode,
    debugOpen,
    live,
    onTool: handleTool,
    onDelete: deleteSelected,
    onDuplicate: duplicateSelected,
    onUndo: undo,
    onRedo: redo,
    onClearSelection: clearSelection,
    onPlay: controls.onPlay,
    onStepTick: debugOpen ? () => debugStep(1) : undefined,
  });

  const effInspector = useMemo(() => {
    if (!live || !inspector || activeId == null) return inspector;
    const sameId = String(inspector.id) === String(activeId);
    const xform = liveXform[activeId];
    const baseT = inspector.transform ?? null;
    const raw = xform
      ? {
          position: xform.position ?? baseT?.position,
          rotation: xform.rotation ?? baseT?.rotation,
          scale: xform.scale ?? baseT?.scale,
        }
      : baseT;
    const transform =
      raw && isQuat(raw.rotation) ? { ...raw, rotation: quatToEulerDeg(raw.rotation) } : raw;
    const liveNames = liveComps[String(activeId)];
    return {
      ...inspector,
      id: String(activeId),
      name: findNodeName(effTree, activeId) ?? (sameId ? inspector.name : `Entity ${activeId}`),
      components: liveNames ?? (sameId ? inspector.components : []),
      transform,
    };
  }, [live, inspector, activeId, liveXform, liveComps, effTree]);

  const nudgeTransform: NudgeFieldFn = (field, axis, delta) => {
    const id = activeIdRef.current;
    if (id == null || !busRef.current) return;
    const sid = String(id);
    let base = nudgeBaseRef.current;
    if (!base || base.id !== sid) {
      const disp = effInspector?.transform ?? null;
      base = {
        id: sid,
        position: { x: 0, y: 0, z: 0, ...(disp?.position ?? {}) },
        euler: { x: 0, y: 0, z: 0, ...(disp?.rotation ?? {}) },
        scale: { x: 1, y: 1, z: 1, ...(disp?.scale ?? {}) },
      };
    }
    const bucket = field === "position" ? base.position : field === "scale" ? base.scale : base.euler;
    bucket[axis] = tidy((Number(bucket[axis]) || 0) + delta);
    nudgeBaseRef.current = base;
    const rotation = eulerDegToQuat(base.euler);
    const cur = compValuesRef.current[sid]?.Transform as { parent?: number } | undefined;
    const engineT: Record<string, unknown> = {
      position: base.position,
      rotation,
      scale: base.scale,
    };
    if (cur && typeof cur.parent === "number") engineT.parent = cur.parent;
    busRef.current.setComponent(sid, "Transform", JSON.stringify(engineT));
    setLiveXform((prev) => ({
      ...prev,
      [sid]: { position: { ...base.position }, rotation, scale: { ...base.scale } },
    }));
  };

  return (
    <DclEditorChrome
      viewportSrc={effViewportSrc}
      viewportRef={viewportRef}
      sceneReady={sceneReady}
      loading={!!viewportSrc && !effViewportSrc}
      loadError={realmStatus === "error"}
      onEngineStatus={handleEngineStatus}
    >
      <DeToolbar
        {...controls}
        live={live}
        showGizmo={!live || sceneReady}
        tool={tool}
        onTool={handleTool}
        camMode={camMode}
        onCamMode={handleCamMode}
        onCameraSettings={live ? () => setCamSettingsOpen(true) : undefined}
        cameraPreset={camPrefs.preset}
        onUndo={undo}
        onRedo={redo}
        hideLeft={hideLeft}
        onToggleLeft={() => setHideLeft((v) => !v)}
        hideRight={hideRight}
        onToggleRight={() => setHideRight((v) => !v)}
        onCode={code ? () => setCodeOpen((v) => !v) : undefined}
        codeActive={codeOpen}
      />
      {!hideLeft &&
        (effLeft === "assets" ? (
          <DeAssetsPanel
            catalog={catalog}
            local={local}
            live={live}
            onPlace={busLive ? placeAsset : undefined}
          />
        ) : (
          <DeHierarchyPanel
            key={live && liveTree != null ? "live-tree" : "seed-tree"}
            title={title}
            tree={effTree}
            live={live}
            onSelect={onHierSelect}
            activeId={activeId}
            onAddEntity={addRootEntity}
            onOpenAssets={() => setAssetsOverride(true)}
          />
        ))}
      {!hideRight && (
        <DeInspectorPanel
          name={effInspector.name}
          id={effInspector.id}
          addOpen={addOpen}
          components={effInspector.components}
          transform={effInspector.transform}
          live={live}
          onAuthorComponent={busLive ? authorComponent : undefined}
          onDeleteComponent={deleteComponent}
          onNudgeTransform={busLive ? nudgeTransform : undefined}
        />
      )}
      {playing && effViewportSrc && engineStatus === "online" && !runPaused && (
        <div className="eui-play-frame" aria-hidden="true" />
      )}
      {playing && effViewportSrc && engineStatus === "online" && (
        <span
          className="eui-play-badge eui-play-badge--preview"
          role="status"
          title="Edits made while the scene is running are temporary — Stop restores the scene to its pre-play state."
        >
          {runPaused ? "❚❚ Paused — edits are temporary" : "● Running — edits are temporary"}
        </span>
      )}
      {debugOpen && effViewportSrc && (
        <DeDebugPanel
          tick={debugUi.tick}
          stepping={debugUi.stepping}
          error={debugUi.error}
          entries={debugUi.entries}
          lastStepCount={debugUi.lastStepCount}
          unchangedEntities={debugUi.unchanged}
          totalEntities={debugUi.total}
          timedOut={debugUi.timedOut}
          systems={debugUi.systems}
          names={(id) =>
            DEBUG_RESERVED_LABELS[id] ?? findNodeName(effTree, id) ?? `Entity ${id}`
          }
          onStep={debugStep}
          onClose={exitDebug}
          onSelect={
            busLive ? (id) => busRef.current?.setSelection([String(id)], String(id)) : undefined
          }
          height={debugHeight}
          onHeightChange={setDebugHeight}
          insetLeft={hideLeft ? 12 : 288}
          insetRight={hideRight ? 12 : 344}
        />
      )}
      {playEditWarn && (
        <PlayEditWarningModal onDismiss={dismissPlayEditWarn} />
      )}
      {codeOpen && code && (
        <Suspense
          fallback={
            <div
              style={{
                position: "absolute",
                inset: "56px 0 0 0",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                background: "#1e1e1e",
                color: "#9a9aa4",
                zIndex: 40,
                pointerEvents: "auto",
              }}
            >
              Loading code editor…
            </div>
          }
        >
          <DeCodeWorkspace code={code} store={codeStoreRef.current} onClose={() => setCodeOpen(false)} />
        </Suspense>
      )}
      {shortcutsOpen && (
        <DeShortcutsOverlay preset={camPrefs.preset} onClose={() => setShortcutsOpen(false)} />
      )}
      {camSettingsOpen && (
        <DeCameraSettings
          prefs={camPrefs}
          onChange={onPrefsChange}
          onReset={onPrefsChange}
          onClose={() => setCamSettingsOpen(false)}
        />
      )}
    </DclEditorChrome>
  );
}
