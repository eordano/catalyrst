import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { EditorTool } from "../bus-protocol";
import type { EditorBus, EditorCamMode } from "../editor-bus";
import { RESTORE_COMPOSITE_TIMEOUT_MS } from "../editor-config";
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
import DclEditorChrome, { type EditorEngineStatus, type EditorBootstrapSnapshot } from "../frames/DclEditorChrome";
import { createHistory, cloneValue, type HistoryEngine } from "../history";
import { forwardEngineKeys } from "../shortcuts";
import { quatToEulerDeg, eulerDegToQuat, isQuat, tidy } from "../transform-nudge";
import { attachCameraInput } from "../camera-input";
import {
  DEFAULT_SNAP,
  loadSnap,
  nextIn,
  saveSnap,
  SNAP_ANGLES,
  SNAP_STEPS,
  type SnapState,
} from "../snap";
import { loadCameraPrefs, saveCameraPrefs } from "../camera-prefs";
import { placeAssetOnBus, registerProjectContents, setProjectPlayState } from "../project-cache";
import { findNodeName } from "../live-tree";
import DeSceneCard, {
  countPlaced,
  sceneMetaWithName,
  readSceneMeta,
  type DeSceneInfo,
} from "../components/DeSceneCard";
import { playBadgeLabel } from "../play-badge";
import DeCameraSettings from "../components/DeCameraSettings";
import DeDebugPanel from "../components/DeDebugPanel";
import DeRenderPanel from "../components/DeRenderPanel";
import { engineConsoleRunner } from "../debugger";
import DeShortcutsOverlay from "../components/DeShortcutsOverlay";
import { DeAssetsPanel, usePlaceStatus, type DeAssetsPreset } from "../components/DeAssetsPanel";
import type { DeInteractionsPreset } from "../components/DeInteractionsPanel";
import { readEntityClipboard, writeEntityClipboard } from "../entity-clipboard";
import type { HierarchyPlacement } from "../hierarchy-selection";
import { folderProject } from "../folder-project";
import type { MaterialChange, MaterialSelection } from "../material-selection";
import type { MaterialValue } from "../gltf-materials";
import { DeAssetCleanup } from "../components/DeAssetCleanup";
import { DeCustomItems } from "../components/DeCustomItems";
import { DeHierarchyPanel } from "../components/DeHierarchyPanel";
import {
  DeInspectorPanel,
  isTransformComp,
  type NudgeFieldFn,
} from "../components/DeInspectorPanel";
import { DeToolbar, type DeToolbarProps } from "../components/DeToolbar";
import DeRibbon from "../components/DeRibbon";
import { McpPairingConsentModal, PlayEditWarningModal } from "../components/DePlayEditWarning";
import { useDebugSession, DEBUG_RESERVED_LABELS } from "./useDebugSession";
import { useEditorBusBridge } from "./useEditorBusBridge";
import { usePlaybackMachine } from "./usePlaybackMachine";
import type { PlaybackCommand } from "../playback-machine";
import { useProjectRealm } from "./useProjectRealm";
import { useSceneMeters } from "./useSceneMeters";
import { useWorkspaceShortcuts } from "./useWorkspaceShortcuts";
import { ACTION_ID, TRIGGER_ID } from "../interactions-vocab";
import { docsUrl } from "../../data/docs";

export { DeToolbar } from "../components/DeToolbar";

const DeUIDesigner = lazy(() => import("../components/DeUIDesigner"));
const DeCodeWorkspace = lazy(() => import("../code/DeCodeWorkspace"));
const SceneAssistantPanel = lazy(() => import("../components/SceneAssistantPanel"));

const PLAY_EDIT_WARNED_KEY = "dcl-editor:play-edit-warned";

const DEFAULT_VEC = { x: 0, y: 0, z: 0 };

const ROOT_SELECTION_HINT =
  "The scene root can\u{2019}t be wired \u{2014} select an item placed in the scene first";
const ROOT_COMMAND_HINTS: Record<string, string> = {
  delete: "The scene root can\u{2019}t be deleted \u{2014} select an item placed in the scene first",
  duplicate:
    "The scene root can\u{2019}t be duplicated \u{2014} select an item placed in the scene first",
  "item.focus":
    "The camera can\u{2019}t focus the scene root \u{2014} select an item placed in the scene first",
};
const SCENE_METADATA_COMPONENT = "inspector::SceneMetadata-v3";

const SAVE_CHIP: Record<string, { label: string; cls: string }> = {
  idle: { label: "Unsaved", cls: "dim" },
  saving: { label: "Saving", cls: "dim" },
  saved: { label: "Saved", cls: "ok" },
  error: { label: "Save failed", cls: "error" },
};

export type { EditorWorkspaceSnapshot } from "../workspace-lifecycle";
import { workspaceSnapshot, type EditorWorkspaceSnapshot } from "../workspace-lifecycle";

interface DeWorkspaceProps {
  operationsBusy?: boolean;
  onLifecycle?: (snapshot: EditorWorkspaceSnapshot) => void;
  renderHeader?: (navigation: ReactNode, scene: { name: string; rename?: (name: string) => void }) => ReactNode;
  left?: "scene" | "assets";
  title?: string;
  onSceneNameChange?: (name: string) => void;
  tree?: DeTreeNode[];
  inspector?: DeInspector;
  addOpen?: boolean;
  catalog?: DeCatalogItem[];
  local?: DeLocalItem[];
  viewportSrc?: string | null;
  rawComposite?: string | null;
  code?: DeWorkspaceCode | null;
  prepareRealm?: ((signal?: AbortSignal) => Promise<unknown>) | null;
  onEngineStatus?: ((status: EditorEngineStatus) => void) | null;
  onSaveToDisk?: (() => void) | null;
  onOpenFromDisk?: (() => void) | null;
  onPublish?: (() => void) | null;
  saveState?: "idle" | "saving" | "saved" | "error";
  sceneInfo?: DeSceneInfo | null;
  onSceneSettings?: () => void;
}

export default function DeWorkspace({
  operationsBusy = false,
  onLifecycle,
  renderHeader,
  left = "scene",
  title = "",
  onSceneNameChange,
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
  onSaveToDisk = null,
  onOpenFromDisk = null,
  onPublish = null,
  saveState = "idle",
  sceneInfo = null,
  onSceneSettings,
}: DeWorkspaceProps) {
  const viewportRef = useRef<HTMLIFrameElement | null>(null);
  const playback = usePlaybackMachine();
  const playing = playback.state.mode !== "editing";
  const runPaused = playback.state.mode === "paused";
  const [playError, setPlayError] = useState<string | null>(null);
  const [mcpConsent, setMcpConsent] = useState<{ host: string; resolve: (ok: boolean) => void } | null>(null);
  const playStateRef = useRef({ playing: false, paused: false });
  playStateRef.current = { playing, paused: runPaused };
  const [playEditWarn, setPlayEditWarn] = useState(false);
  const playEditNotedRef = useRef(false);
  const hasEditedRef = useRef(false);
  const [hasEdited, setHasEdited] = useState(false);
  const noteEdit = () => {
    if (hasEditedRef.current) return;
    hasEditedRef.current = true;
    setHasEdited(true);
  };
  const notePlayEdit = () => {
    noteEdit();
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
  const [snap, setSnap] = useState<SnapState>(() => loadSnap());
  const snapRef = useRef<SnapState>(snap);
  snapRef.current = snap;
  const applySnap = (next: SnapState) => setSnap(saveSnap(next));
  const [codeOpen, setCodeOpen] = useState(false);
  const [uiDesignerOpen, setUIDesignerOpen] = useState(false);
  const [assistantOpen, setAssistantOpen] = useState(false);
  const assistantBridgeRef = useRef<(() => void) | null>(null);
  useEffect(() => () => assistantBridgeRef.current?.(), [code?.project?.id]);
  const pairAssistant = async (url: string) => {
    const project = code?.project;
    if (!project) throw new Error("Open an SDK project before pairing scene tools.");
    const expected = new URL(`${project.id.replace(/\/$/, "")}/api/project/assistant/bridge`);
    expected.protocol = expected.protocol === "https:" ? "wss:" : "ws:";
    if (new URL(url).href !== expected.href) throw new Error("The scene tools bridge must belong to this SDK project.");
    const { connect } = await import("../mcp-bridge");
    assistantBridgeRef.current?.();
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        assistantBridgeRef.current?.();
        reject(new Error("The scene tools server did not pair. Check the SDK's scene tools configuration and retry."));
      }, 15000);
      assistantBridgeRef.current = connect({ url, token: "sdk-project", getViewportEl: () => viewportRef.current,
        onPairingChange: (paired, error) => {
          if (paired) { clearTimeout(timer); resolve(); }
          else if (error) { clearTimeout(timer); reject(new Error(error)); }
        },
      });
    });
  };
  const [tool, setTool] = useState<EditorTool>("translate");
  const [hideLeft, setHideLeft] = useState(false);
  const [hideRight, setHideRight] = useState(false);
  const codeStoreRef = useRef<Map<string, string>>(new Map());

  const realmStatus = useProjectRealm(viewportSrc, prepareRealm);
  const [retrySession, setRetrySession] = useState<string | null>(null);
  const activeViewportSrc = useMemo(() => {
    if (!viewportSrc || !retrySession) return viewportSrc;
    const url = new URL(viewportSrc, typeof window === "undefined" ? "https://editor.invalid" : window.location.href);
    url.searchParams.set("editorSession", retrySession);
    return url.toString();
  }, [viewportSrc, retrySession]);
  const effViewportSrc = realmStatus === "ready" ? activeViewportSrc : null;

  const [bootstrap, setBootstrap] = useState<EditorBootstrapSnapshot>({ ready: false, error: null, progress: null, stage: null });
  const [engineStatus, setEngineStatus] = useState<EditorEngineStatus>("connecting");
  const onEngineStatusRef = useRef(onEngineStatus);
  onEngineStatusRef.current = onEngineStatus;
  const handleEngineStatus = useCallback((s: EditorEngineStatus) => {
    setEngineStatus(s);
    onEngineStatusRef.current?.(s);
  }, []);

  const live = !!effViewportSrc;
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
  } = useDebugSession({ viewportRef, busRef, playStateRef, pausePlayback: () => playback.current.current.mode === "paused" ? Promise.resolve(true) : runPlayback("pause") });

  const runPlayback = (command: PlaybackCommand): Promise<boolean> => {
    const bus = busRef.current;
    const request = playback.begin(command, !!bus && sceneReady && engineStatus === "online" && !operationsBusy);
    if (!request || !bus) return Promise.resolve(false);
    const current = () => busRef.current === bus && playback.accepts(request);
    setPlayError(null);
    return (async () => {
      if (command === "play") {
        if (playback.current.current.mode === "editing") {
          try {
            const composite = await bus.exportComposite();
            if (!current()) return false;
            if (typeof composite !== "string" || !composite.trim()) throw new Error("Missing scene snapshot");
            prePlayRef.current = composite;
          } catch {
            throw new Error("The editor could not preserve your scene before Play. Try again once the scene is connected.");
          }
        }
        await setProjectPlayState(true);
        if (!current()) return false;
        await bus.rpc("setPlayback", [true, false]);
      } else if (command === "pause" || command === "suspend" || command === "resume") {
        await bus.rpc("setPlayback", [true, command !== "resume"]);
      } else if (command === "step") {
        await bus.rpc("stepPlayback", [1]);
      } else {
        const snapshot = prePlayRef.current;
        if (!snapshot) throw new Error("The scene snapshot is unavailable. Reconnect the editor before continuing.");
        if (debugOpenRef.current) exitDebug(false);
        await setProjectPlayState(false);
        if (!current()) return false;
        await bus.rpc("stopPlayback", [snapshot], 45000);
      }
      if (!current()) return false;
      const next = playback.send({ type: "completed", request });
      const running = next.mode !== "editing";
      const paused = next.mode === "paused";
      playStateRef.current = { playing: running, paused };
      if (command === "play") {
        if (debugOpenRef.current) exitDebug();
        playEditNotedRef.current = false;
        viewportRef.current?.focus();
      }
      if (command === "stop") {
        prePlayRef.current = null;
        playEditNotedRef.current = false;
        setPlayEditWarn(false);
      }
      if (command !== "step" && command !== "suspend" && command !== "resume") bus.announcePlayState(running, paused);
      return true;
    })().catch(async (error: unknown) => {
      if (!current()) return false;
      if (command === "play" && playback.current.current.mode === "editing") await setProjectPlayState(false);
      if (command === "stop") await setProjectPlayState(true);
      if (!current()) return false;
      const message = command === "stop"
        ? "The scene could not be restored. Try Stop again before making further edits."
        : command === "pause" ? "The scene could not be paused. Try Pause again."
        : error instanceof Error ? error.message : String(error);
      playback.send({ type: "failed", request, error: message });
      setPlayError(message);
      return false;
    });
  };
  const rawCompositeRef = useRef<string | null>(rawComposite);
  rawCompositeRef.current = rawComposite;
  const [camMode, setCamMode] = useState<EditorCamMode>("target");
  const [assetsOverride, setAssetsOverride] = useState<boolean | null>(null);
  const [camPrefs, setCamPrefs] = useState<CameraPrefs>(() => loadCameraPrefs());
  const [camSettingsOpen, setCamSettingsOpen] = useState(false);
  const [renderTuningOpen, setRenderTuningOpen] = useState(false);
  const [cameraHintDismissed, setCameraHintDismissed] = useState<boolean>(() => {
    try {
      return window.localStorage?.getItem("eui-camera-hint-dismissed") === "1";
    } catch {
      return false;
    }
  });
  const dismissCameraHint = () => {
    setCameraHintDismissed(true);
    try {
      window.localStorage?.setItem("eui-camera-hint-dismissed", "1");
    } catch {
    }
  };
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
  const applyHistoryWriteRef = useRef<(entity: string, name: string, value: unknown) => Promise<void>>(
    async () => {},
  );
  const historyRef = useRef<HistoryEngine | null>(null);
  const hierarchyPendingRef = useRef(false);
  if (historyRef.current === null) {
    historyRef.current = createHistory(
      (entity, name, value) => applyHistoryWriteRef.current(entity, name, value),
      () => setHistoryVersion((v) => v + 1),
    );
  }
  const history = historyRef.current;

  const [folderFiles, setFolderFiles] = useState<NonNullable<DeWorkspaceCode["project"]> | null>(null);
  const folderPromise = useRef<Promise<NonNullable<DeWorkspaceCode["project"]> | null> | null>(null);
  useEffect(() => {
    let current = true;
    setFolderFiles(null);
    const pending = !code?.project && code?.getDir
      ? code.getDir().then(directory => directory ? folderProject(directory) : null)
      : Promise.resolve(null);
    folderPromise.current = pending;
    void pending.then(project => { if (current) setFolderFiles(project); }).catch(() => {});
    return () => { current = false; };
  }, [code]);
  const projectFiles = code?.project ?? folderFiles;
  const prepareScene = code?.getDir || code?.project?.assets?.preparePreview ? async () => {
    const project = code?.project ?? await folderPromise.current;
    await registerProjectContents(busRef, project?.assets);
  } : undefined;

  const {
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
  } = useEditorBusBridge({
    prepareScene,
    live,
    viewportSrc: effViewportSrc,
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
  });
  activeIdRef.current = liveSel?.active ?? null;

  useEffect(() => {
    playback.send({ type: "reset" });
    void setProjectPlayState(false);
    exitDebug(false);
    playStateRef.current = { playing: false, paused: false };
    setPlayError(null);
    prePlayRef.current = null;
    return () => { void setProjectPlayState(false); };
  }, [effViewportSrc, session, playback.send]);

  const snapshotSequence = useRef(0);
  const lifecycle = useMemo(() => workspaceSnapshot({
    session, sequence: 0, destination: effViewportSrc, project: realmStatus,
    engine: bootstrap, scene: { ready: sceneReady, error: sceneError }, playback: playback.state, operationsBusy,
  }), [session, effViewportSrc, realmStatus, bootstrap, sceneReady, sceneError, playback.state, operationsBusy]);
  const { capabilities } = lifecycle;
  const controls: Partial<DeToolbarProps> = live ? {
    playing: playing && !runPaused,
    onPlay: capabilities.play ? () => runPlayback("play") : undefined,
    onPause: capabilities.pause ? () => runPlayback("pause") : undefined,
    onStep: capabilities.step ? () => debugOpenRef.current ? debugStep(1) : runPlayback("step") : undefined,
    onStop: capabilities.stop ? () => runPlayback("stop") : undefined,
    onDebug: capabilities.debug ? () => debugOpenRef.current ? exitDebug() : void enterDebug() : undefined,
    debugActive: debugOpen,
  } : {};
  useEffect(() => {
    onLifecycle?.({ ...lifecycle, sequence: ++snapshotSequence.current });
  }, [lifecycle, onLifecycle]);



  applyHistoryWriteRef.current = async (entity, name, value) => {
    notePlayEdit();
    const bus = busRef.current;
    if (!bus) throw new Error("Reconnect the editor before restoring history.");
    if (playStateRef.current.playing) throw new Error("Stop the preview before restoring history.");
    if (name === "@scene") {
      if (typeof value !== "string") throw new Error("The scene history is invalid.");
      await bus.rpc("restoreComposite", [value], RESTORE_COMPOSITE_TIMEOUT_MS);
      return;
    }
    const key = String(entity);
    if (value === undefined) {
      await bus.rpc("removeComponent", [String(entity), name]);
      const vals = compValuesRef.current[key];
      if (vals) delete vals[name];
      setLiveComps((prev) => {
        const cur = prev[key];
        return cur ? { ...prev, [key]: cur.filter((c) => c !== name) } : prev;
      });
      return;
    }
    await bus.rpc("writeComponents", [String(entity), [{ name, value }]]);
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
    if (!effViewportSrc || typeof window === "undefined") return undefined;
    const optedIn =
      /[?&]mcp=/.test(window.location.search) || !!window.localStorage?.getItem("dcl-mcp-relay");
    if (!optedIn) return undefined;
    let gone = false;
    let dispose: (() => void) | null = null;
    import("../mcp-bridge")
      .then((m) => {
        if (gone) return;
        dispose = m.autoConnect({
          getViewportEl: () => viewportRef.current,
          confirmRemote: (host) =>
            new Promise<boolean>((resolve) => {
              setMcpConsent({ host, resolve });
            }),
        });
      })
      .catch(() => {});
    return () => {
      gone = true;
      dispose?.();
    };
  }, [effViewportSrc]);

  const runPlaybackRef = useRef(runPlayback);
  runPlaybackRef.current = runPlayback;
  useEffect(() => {
    if (!live || !sceneReady || typeof document === "undefined") return undefined;
    const onVisibility = () => {
      const state = playback.current.current;
      if (state.mode !== "playing" || state.pending || state.outcome?.error) return;
      if (document.visibilityState === "hidden" && !state.suspended) void runPlaybackRef.current("suspend");
      else if (document.visibilityState !== "hidden" && state.suspended) void runPlaybackRef.current("resume");
    };
    onVisibility();
    document.addEventListener("visibilitychange", onVisibility);
    return () => document.removeEventListener("visibilitychange", onVisibility);
  }, [live, sceneReady, session, playback.state]);

  useEffect(() => {
    if (!live || !sceneReady) return undefined;
    const cw = viewportRef.current?.contentWindow;
    const bus = busRef.current;
    if (!cw || !bus) return undefined;
    const detach = attachCameraInput(
      cw,
      bus,
      () => prefsRef.current,
      () => ({ camMode: camModeRef.current, activeId: activeIdRef.current, playing: playStateRef.current.playing }),
    );
    const detachKeys = forwardEngineKeys(cw, {
      iframe: viewportRef.current,
      isEditingEnabled: () => !playStateRef.current.playing &&
        camModeRef.current !== "free" && !cw.document.pointerLockElement,
    });
    bus.setCamMode(camModeRef.current);
    return () => { detach(); detachKeys(); };
  }, [live, sceneReady, session]);

  const handleTool = (t: EditorTool) => {
    setTool(t);
    busRef.current?.setTool(t);
  };


  const authorComponents = async (entity: string | number | null | undefined, changes: { name: string; json: string }[]) => {
    const bus = busRef.current;
    if (!bus || entity == null) throw new Error("Reconnect the editor before changing components.");
    if (playing) throw new Error("Stop the preview before changing components.");
    if (history.isSuppressed() || hierarchyPendingRef.current) throw new Error("Wait for the current edit to finish before changing components.");
    const key = String(entity);
    const updates = changes.map(change => ({ name: change.name, value: JSON.parse(change.json) as unknown }));
    const batch = updates.map(change => ({ entity: key, name: change.name, before: cloneValue(compValuesRef.current[key]?.[change.name]), after: cloneValue(change.value) }));
    await bus.rpc("writeComponents", [key, updates]);
    if (busRef.current !== bus) throw new Error("The editor reconnected while saving. Check the component values.");
    notePlayEdit();
    historyRef.current?.push(batch);
    for (const update of updates) (compValuesRef.current[key] ??= {})[update.name] = update.value;
    setLiveComps(prev => ({ ...prev, [key]: [...new Set([...(prev[key] ?? []), ...updates.map(update => update.name)])] }));
    const transform = updates.find(update => isTransformComp(update.name));
    if (transform) setLiveXform(prev => ({ ...prev, [key]: transform.value as EditorTransform }));
  };
  const authorMaterialSelection = async (changes: MaterialChange[]) => {
    const bus = busRef.current;
    if (!bus) throw new Error("Reconnect the editor before changing materials.");
    if (playStateRef.current.playing) throw new Error("Stop the preview before changing materials.");
    if (history.isSuppressed() || hierarchyPendingRef.current) throw new Error("Wait for the current edit to finish before changing materials.");
    hierarchyPendingRef.current = true;
    try {
      await bus.rpc("writeMaterials", [changes], 90000);
      if (busRef.current !== bus) throw new Error("The editor reconnected while saving. Check the selected materials.");
      historyRef.current?.push(changes.map(change => ({ entity: change.entity, name: change.name, before: cloneValue(change.before), after: cloneValue(change.value) })));
      for (const change of changes) (compValuesRef.current[change.entity] ??= {})[change.name] = cloneValue(change.value);
      notePlayEdit();
      setLiveComps(previous => ({ ...previous }));
    } finally { hierarchyPendingRef.current = false; }
  };
  const authorComponent: AuthorComponentFn = (entity, name, json) => {
    void authorComponents(entity, [{ name, json }]).catch(error => setPlayError(error instanceof Error ? error.message : "The component could not be changed. Try again."));
  };

  const placeAsset = (asset: DeCatalogItem, drop?: { x: number; y: number } | null) => {
    notePlayEdit();
    return placeAssetOnBus(busRef, asset, drop ?? null, projectFiles?.assets);
  };

  const busConnected = live && sceneReady && engineStatus === "online";
  const busLive = busConnected && !playback.state.pending && !operationsBusy;
  const [writableComponents, setWritableComponents] = useState<ReadonlySet<string>>(new Set());
  useEffect(() => {
    let current = true;
    setWritableComponents(new Set());
    if (busConnected) {
      const run = engineConsoleRunner(viewportRef.current);
      void (run ? run("/component_names") : Promise.reject(new Error("Engine console unavailable")))
        .then(result => {
          const names: unknown = JSON.parse(result);
          if (!Array.isArray(names) || !names.every(name => typeof name === "string")) throw new Error("Invalid component permissions");
          if (current) setWritableComponents(new Set(names));
        })
        .catch(() => { if (current) setPlayError("Could not load component editing permissions. Reopen the editor to retry."); });
    }
    return () => { current = false; };
  }, [busConnected, session]);
  const { status: placeStatus, place: placeTracked } = usePlaceStatus(
    busLive ? placeAsset : undefined,
  );

  const [dragAsset, setDragAsset] = useState<DeCatalogItem | null>(null);
  const handleViewportDrop = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    const asset = dragAsset;
    setDragAsset(null);
    if (!asset || !placeTracked) return;
    const el = viewportRef.current;
    const r = el?.getBoundingClientRect();
    if (!r || r.width <= 0 || r.height <= 0) {
      placeTracked(asset);
      return;
    }
    const x = Math.min(Math.max((e.clientX - r.left) / r.width, 0), 1);
    const y = Math.min(Math.max((e.clientY - r.top) / r.height, 0), 1);
    placeTracked(asset, { x, y });
  };
  const activeId = liveSel?.active ?? null;
  const onHierSelection = busLive && !playing
    ? (ids: string[], active: string | null) => {
        busRef.current?.setSelection(ids, active);
        setLiveSel({ selected: ids, active });
      }
    : undefined;
  const authorHierarchy = async (method: string, args: unknown[]) => {
    const bus = busRef.current;
    if (!bus || playing) throw new Error("Stop the preview and reconnect before editing the hierarchy.");
    if (history.isSuppressed()) throw new Error("Wait for Undo or Redo to finish before editing the hierarchy.");
    if (hierarchyPendingRef.current) throw new Error("Wait for the current hierarchy edit to finish.");
    hierarchyPendingRef.current = true;
    try {
      const before = await bus.exportComposite();
      if (typeof before !== "string") throw new Error("The scene could not be saved for Undo.");
      let result: unknown;
      try { result = await bus.rpc(method, args); }
      catch (error) {
        try { await bus.rpc("restoreComposite", [before], RESTORE_COMPOSITE_TIMEOUT_MS); }
        catch { throw new Error(`The operation failed and the scene could not be restored. Reconnect and inspect the scene. ${String(error)}`); }
        throw error;
      }
      if (bus !== busRef.current) throw new Error("The editor reconnected during the operation.");
      const after = await bus.exportComposite();
      if (typeof after !== "string") throw new Error("The updated scene could not be saved for Undo.");
      history.push([{ entity: "0", name: "@scene", before, after }]);
      return result;
    } finally { hierarchyPendingRef.current = false; }
  };
  const onHierarchyMove = busLive && !playing
    ? async (ids: string[], target: string, placement: HierarchyPlacement) => {
        const bus = busRef.current;
        if (!bus) throw new Error("Reconnect the editor before moving entities.");
        await authorHierarchy("moveHierarchy", [ids, target, placement]);
        if (busRef.current !== bus) return;
        onHierSelection?.(ids, ids[0] ?? null);
      }
    : undefined;

  const handleCamMode = busLive && !playing
    ? (m: EditorCamMode) => {
        setCamMode(m);
        busRef.current?.setCamMode(m);
      }
    : undefined;
  const deleteComponent: DeleteComponentFn | undefined = busLive
    ? (entity, name) => {
        notePlayEdit();
        const bus = busRef.current;
        if (!bus || history.isSuppressed() || hierarchyPendingRef.current) return;
        const key = String(entity);
        const before = cloneValue(compValuesRef.current[key]?.[name]);
        void bus.rpc("removeComponent", [key, name]).then(() => {
          if (bus !== busRef.current) return;
          if (before !== undefined) historyRef.current?.push([{ entity: key, name, before, after: undefined }]);
          const values = compValuesRef.current[key];
          if (values) delete values[name];
          setLiveComps(previous => previous[key] ? { ...previous, [key]: previous[key].filter(component => component !== name) } : previous);
        }).catch(error => setPlayError(error instanceof Error ? error.message : "The component could not be removed."));
      }
    : undefined;
  const addRootEntity = busLive
    ? () => {
        notePlayEdit();
        void authorHierarchy("addEntity", ["Entity", 0, null, null]).catch(error => setPlayError(error instanceof Error ? error.message : "The scene could not create the entity."));
      }
    : undefined;
  const sceneTitle = readSceneMeta(compValuesRef.current["0"]?.[SCENE_METADATA_COMPONENT]).name || title || liveScene?.title || "Untitled scene";
  useEffect(() => { onSceneNameChange?.(sceneTitle); }, [sceneTitle, onSceneNameChange]);
  const renameScene = busLive
    ? (name: string) =>
        authorComponent(
          "0",
          SCENE_METADATA_COMPONENT,
          JSON.stringify(
            sceneMetaWithName(compValuesRef.current["0"]?.[SCENE_METADATA_COMPONENT], name, sceneInfo),
          ),
        )
    : undefined;
  const restoreHistory = (direction: "undo" | "redo") => {
    if (hierarchyPendingRef.current) { setPlayError("Wait for the current hierarchy edit to finish."); return; }
    void history[direction]().catch(error => setPlayError(error instanceof Error ? error.message : "History could not be restored."));
  };
  const undo = busLive && history.canUndo() ? () => restoreHistory("undo") : undefined;
  const redo = busLive && history.canRedo() ? () => restoreHistory("redo") : undefined;
  const effLeft = assetsOverride === null ? left : assetsOverride ? "assets" : "scene";
  const showScene = busLive ? () => setAssetsOverride(false) : undefined;

  const effTree = live && liveTree != null ? liveTree : tree;
  const sceneEmpty = countPlaced(effTree) === 0;
  const activeName = useMemo(
    () => (activeId != null ? findNodeName(effTree, activeId) : null),
    [effTree, activeId],
  );

  const selectedIds: string[] = liveSel?.selected?.length
    ? liveSel.selected
    : activeId != null
      ? [String(activeId)]
      : [];
  const materialSelection: MaterialSelection[] = [...selectedIds].sort((a, b) => Number(b === String(activeId)) - Number(a === String(activeId))).map(entity => {
    const values = compValuesRef.current[entity] ?? {};
    const name = values.Material !== undefined ? "Material" : "core::Material";
    return { entity, name, value: values[name] as MaterialValue | undefined };
  });
  const rootActive =
    activeId != null ? String(activeId) === "0" : !live && String(inspector.id ?? "") === "0";
  const placeableIds = selectedIds.filter((id) => String(id) !== "0");
  const deleteSelected = busLive && !playing && placeableIds.length > 0
    ? () => {
        notePlayEdit();
        void authorHierarchy("removeEntities", [placeableIds]).catch(error => setPlayError(error instanceof Error ? error.message : "The selection could not be removed."));
      }
    : undefined;
  const duplicateHierarchy = busLive && !playing && placeableIds.length
    ? async () => {
        const bus = busRef.current;
        if (!bus) throw new Error("Reconnect the editor before duplicating entities.");
        const payload = await bus.rpc("copyEntities", [placeableIds]);
        if (busRef.current !== bus) throw new Error("The editor reconnected. Duplicate again.");
        const parent = String((compValuesRef.current[placeableIds[0] ?? ""]?.Transform as { parent?: number } | undefined)?.parent ?? 0);
        await authorHierarchy("pasteEntities", [payload, parent]);
      }
    : undefined;
  const duplicateSelected = duplicateHierarchy ? () => {
    void duplicateHierarchy().catch(error => setPlayError(error instanceof Error ? error.message : "The selection could not be duplicated. Try again."));
  } : undefined;
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
      name: activeName ?? (sameId ? inspector.name : `Entity ${activeId}`),
      components: liveNames ?? (sameId ? inspector.components : []),
      transform,
    };
  }, [live, inspector, activeId, liveXform, liveComps, activeName]);

  const writeTransform = (
    field: "position" | "rotation" | "scale",
    axis: keyof EditorVec,
    next: (current: number) => number,
  ) => {
    const id = activeIdRef.current;
    if (id == null || !busRef.current) return;
    noteEdit();
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
    bucket[axis] = tidy(next(Number(bucket[axis]) || 0));
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

  const nudgeTransform: NudgeFieldFn = (field, axis, delta) =>
    writeTransform(field, axis, (cur) => cur + delta);

  const setTransformAxis = (
    field: "position" | "rotation",
    axis: "x" | "y" | "z",
    value: number,
  ) => writeTransform(field, axis, () => value);

  const [assetsPreset, setAssetsPreset] = useState<DeAssetsPreset | null>(null);
  const [interPreset, setInterPreset] = useState<DeInteractionsPreset | null>(null);
  const [reveal, setReveal] = useState<{ target: "add" | "wire"; n: number } | null>(null);
  const nonceRef = useRef(0);
  const nextNonce = () => (nonceRef.current += 1);

  const openAssets = (p: Omit<DeAssetsPreset, "nonce"> = {}) => {
    setHideLeft(false);
    setAssetsOverride(true);
    setAssetsPreset({ nonce: nextNonce(), tab: "catalog", ...p });
  };
  const revealPanel = (target: "add" | "wire") => {
    setHideRight(false);
    setReveal({ target, n: nextNonce() });
  };
  const wireTo = (p: { trigger?: string; action?: string } = {}) => {
    revealPanel("wire");
    setInterPreset({ nonce: nextNonce(), ...p });
  };

  const meters = useSceneMeters({ busRef, busLive, scene: liveScene });

  const wiring = useMemo(() => {
    if (!busLive) return undefined;
    const vals = activeId == null ? {} : (compValuesRef.current[String(activeId)] ?? {});
    const triggers = vals["asset-packs::Triggers"] as
      | { value?: Array<{ type?: string }> }
      | undefined;
    const actions = vals["asset-packs::Actions"] as
      | { value?: Array<{ type?: string }> }
      | undefined;
    return {
      smart: Object.keys(vals).some((k) => k.startsWith("asset-packs::")),
      wired: (triggers?.value?.length ?? 0) > 0,
      trigger: triggers?.value?.[0]?.type ?? null,
      action: actions?.value?.[0]?.type ?? null,
    };
  }, [busLive, activeId, liveComps]);

  const selectionLabel =
    selectedIds.length > 1
      ? `${selectedIds.length} items`
      : activeId != null
        ? (activeName ?? `Entity ${activeId}`)
        : "Selection";

  const toggleAlignWorld = () => {
    const next = !orientGlobal;
    setOrientGlobal(next);
    busRef.current?.setFlags({ orientGlobal: next });
  };

  const openDocs = (path: string) => {
    if (typeof window === "undefined") return;
    window.open(path, "_blank", "noopener,noreferrer");
  };

  const ribbonCommands: Record<string, (() => void) | undefined> = {
    undo: busLive ? () => restoreHistory("undo") : undefined,
    redo: busLive ? () => restoreHistory("redo") : undefined,
    duplicate: () => duplicateSelected?.(),
    delete: () => deleteSelected?.(),
    "tool.translate": () => handleTool("translate"),
    "tool.rotate": () => handleTool("rotate"),
    "tool.scale": () => handleTool("scale"),
    "tool.select": () => handleTool("select"),
    snap: () => applySnap({ ...snap, on: !snap.on }),
    "snap.step": () => applySnap({ ...snap, step: nextIn(SNAP_STEPS, snap.step) }),
    "snap.angle": () => applySnap({ ...snap, angle: nextIn(SNAP_ANGLES, snap.angle) }),
    "align.world": toggleAlignWorld,
    "item.focus": playing ? undefined : () => {
      if (activeId != null) busRef.current?.focus(String(activeId), true);
    },
    "item.inspector": () => setHideRight(false),
    "item.addComponent": () => revealPanel("add"),
    "assets.open": () => openAssets({ cat: "", smart: false }),
    "assets.search": () => openAssets({ focusSearch: true }),
    "smart.doors": () => openAssets({ cat: "doors", smart: true, query: "" }),
    "smart.buttons": () => openAssets({ cat: "buttons", smart: true, query: "" }),
    "smart.platforms": () => openAssets({ cat: "platforms", smart: true, query: "" }),
    "smart.seats": () => openAssets({ cat: "seats", smart: true, query: "" }),
    "smart.all": () => openAssets({ cat: "", smart: true, query: "" }),
    "entity.new": addRootEntity,
    import: () => openAssets({ tab: "local" }),
    "wire.quick": () => wireTo(),
    "trigger.click": () => wireTo({ trigger: TRIGGER_ID.click }),
    "trigger.input": () => wireTo({ trigger: TRIGGER_ID.input }),
    "action.tween": () => wireTo({ action: ACTION_ID.tween }),
    "action.visibility": () => wireTo({ action: ACTION_ID.visibility }),
    "action.sound": () => wireTo({ action: ACTION_ID.sound }),
    "action.animate": () => wireTo({ action: ACTION_ID.animate }),
    play: controls.onPlay,
    pause: () => controls.onPause?.(),
    step: () => controls.onStep?.(),
    stop: () => controls.onStop?.(),
    debug: () => controls.onDebug?.(),
    code: code ? () => setCodeOpen((v) => !v) : undefined,
    agent: code?.project?.assistant ? () => setAssistantOpen((value) => !value) : undefined,
    "render.tuning": () => setRenderTuningOpen((v) => !v),
    "ref.docs": () => openDocs(docsUrl("creator")),
    "ref.playground": () => openDocs("https://playground.decentraland.org/"),
    save: onSaveToDisk ?? undefined,
    open: onOpenFromDisk ?? undefined,
    publish: onPublish ?? undefined,
    "scene.settings": onSceneSettings,
  };

  const saveChip = SAVE_CHIP[saveState] ?? SAVE_CHIP.idle!;
  const chipHidden = saveState === "idle" && !hasEdited && !playing;
  const answerMcpConsent = (approved: boolean) => {
    mcpConsent?.resolve(approved);
    setMcpConsent(null);
  };

  return (
    <DclEditorChrome
      viewportSrc={effViewportSrc}
      viewportRef={viewportRef}
      sceneReady={sceneReady}
      sceneError={sceneError}
      onViewportLoad={onViewportLoad}
      loading={!!viewportSrc && !effViewportSrc}
      loadError={realmStatus === "error"}
      onRetry={() => setRetrySession(crypto.randomUUID())}
      onBootstrap={setBootstrap}
      onEngineStatus={handleEngineStatus}
    >
      {playError && <div className="eui-boot eui-boot--error" role="alert">
        <span>{playError}</span>
        <button className="eui-btn" type="button" onClick={() => setPlayError(null)}>Dismiss</button>
      </div>}
      {
}
      <DeRibbon
        onTab={(t) => {
          if (t !== "insert") showScene?.();
        }}
        hasSelection={placeableIds.length > 0}
        selectionLabel={selectionLabel}
        selectionHint={
          rootActive && placeableIds.length === 0 ? ROOT_SELECTION_HINT : undefined
        }
        selectionHints={
          rootActive && placeableIds.length === 0 ? ROOT_COMMAND_HINTS : undefined
        }
        busLive={busLive}
        renderHeader={renderHeader ? navigation => renderHeader(navigation, {
          name: sceneTitle,
          rename: playing ? undefined : renameScene,
        }) : undefined}
        saveLabel={chipHidden ? "" : saveChip.label}
        saveClass={saveChip.cls}
        playing={playing}
        canUndo={history.canUndo()}
        canRedo={history.canRedo()}
        meters={meters}
        cameraPose={cameraPose ?? undefined}
        snapLabel={
          snap.on
            ? `snap ${snap.step} m \u{00B7} ${snap.angle}\u{00B0}`
            : "snap off"
        }
        pressed={{
          "tool.translate": tool === "translate",
          "tool.rotate": tool === "rotate",
          "tool.scale": tool === "scale",
          "tool.select": tool === "select",
          snap: snap.on,
          "align.world": orientGlobal,
          debug: debugOpen,
          "render.tuning": renderTuningOpen,
        }}
        labels={{
          "snap.step": `${snap.step} m`,
          "snap.angle": `${snap.angle}\u{00B0}`,
        }}
        numeric={
          busLive
            ? {
                position: effInspector.transform?.position ?? DEFAULT_VEC,
                rotation: effInspector.transform?.rotation ?? DEFAULT_VEC,
                step: snap.on ? snap.step : DEFAULT_SNAP.step,
                angleStep: snap.on ? snap.angle : DEFAULT_SNAP.angle,
                onCommit: setTransformAxis,
              }
            : undefined
        }
        wiring={wiring}
        onOpenWiring={busLive ? () => revealPanel("wire") : undefined}
        commands={ribbonCommands}
      />
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
        hideLeft={hideLeft}
        onToggleLeft={() => setHideLeft((v) => !v)}
        hideRight={hideRight}
        onToggleRight={() => setHideRight((v) => !v)}
        onCode={code ? () => { setUIDesignerOpen(false); setCodeOpen((v) => !v); } : undefined}
        codeActive={codeOpen}
        onUIDesigner={code?.project?.uiDesigner ? () => { setCodeOpen(false); setUIDesignerOpen(value => !value); } : undefined}
        uiDesignerActive={uiDesignerOpen}
      />
      {!hideLeft &&
        (effLeft === "assets" ? (
          <DeAssetsPanel
            cleanup={projectFiles?.assets ? <DeAssetCleanup project={projectFiles} exportComposite={busLive && !playing ? async () => {
              const composite = await busRef.current?.exportComposite();
              if (typeof composite !== "string") throw new Error("The current scene could not be inspected. Reconnect and try again.");
              return composite;
            } : undefined} /> : undefined}
            customItems={code ? <DeCustomItems code={code} selectionName={activeName}
              onCapture={busLive && !playing && placeableIds.length ? async () => {
                const bus = busRef.current;
                if (!bus) throw new Error("Reconnect the editor before saving a custom item.");
                return bus.rpc("copyEntities", [placeableIds]);
              } : undefined}
              onPlace={busLive && !playing ? async payload => {
                const bus = busRef.current;
                if (!bus) throw new Error("Reconnect the editor before placing a custom item.");
                await authorHierarchy("pasteEntities", [payload, "0"]);
              } : undefined}
            /> : undefined}
            catalog={catalog}
            local={local}
            live={live}
            preset={assetsPreset}
            onBack={() => setAssetsOverride(false)}
            onPlace={placeTracked}
            placeStatus={placeStatus}
            onDragAsset={busLive ? setDragAsset : undefined}
          />
        ) : (
          <DeHierarchyPanel
            key={live && liveTree != null ? "live-tree" : "seed-tree"}
            title={title}
            tree={effTree}
            live={live}
            onSelectionChange={onHierSelection}
            selectedIds={selectedIds}
            onMove={onHierarchyMove}
            onCopy={busLive && !playing && placeableIds.length ? async () => {
              const bus = busRef.current;
              if (!bus) throw new Error("Reconnect the editor before copying entities.");
              await writeEntityClipboard(await bus.rpc("copyEntities", [placeableIds]));
            } : undefined}
            onPaste={busLive && !playing ? async () => {
              const bus = busRef.current;
              if (!bus) throw new Error("Reconnect the editor before pasting entities.");
              const payload = await readEntityClipboard();
              if (busRef.current !== bus) throw new Error("The editor reconnected. Paste again.");
              await authorHierarchy("pasteEntities", [payload, activeId ?? "0"]);
            } : undefined}
            onDuplicate={duplicateHierarchy}
            onUnparent={onHierarchyMove && placeableIds.length ? () => onHierarchyMove(placeableIds, "0", "inside") : undefined}
            onFocus={
              busLive
                ? (id) => {
                    busRef.current?.setSelection([String(id)], String(id));
                    busRef.current?.focus(String(id), true, true);
                  }
                : undefined
            }
            activeId={activeId}
            onAddEntity={addRootEntity}
            onOpenAssets={() => setAssetsOverride(true)}
          />
        ))}
      {!hideRight && rootActive && (
        <DeSceneCard
          title={title}
          info={sceneInfo}
          live={liveScene ?? null}
          metadata={compValuesRef.current["0"]?.[SCENE_METADATA_COMPONENT]}
          tree={effTree}
          onRename={renameScene}
        />
      )}
      {!hideRight && !rootActive && (
        <DeInspectorPanel
          assets={projectFiles?.assets}
          materialSelection={materialSelection}
          onAuthorMaterialSelection={busLive && !playing ? authorMaterialSelection : undefined}
          clipboard={busLive && !playing ? {
            copy: (entity, name) => busRef.current!.rpc("copyComponent", [String(entity), name]),
            paste: (name, payload) => busRef.current!.rpc("pasteComponent", [selectedIds.map(String), name, payload]),
          } : undefined}
          name={effInspector.name}
          id={effInspector.id}
          addOpen={addOpen || reveal?.target === "add"}
          interactionsOpen={reveal?.target === "wire"}
          revealNonce={reveal?.n ?? 0}
          interactionsPreset={interPreset}
          components={effInspector.components}
          writableComponents={live ? writableComponents : undefined}
          componentValues={
            activeId != null ? compValuesRef.current[String(activeId)] : undefined
          }
          transform={effInspector.transform}
          live={live}
          onAuthorComponent={busLive && !playing ? authorComponent : undefined}
          onAuthorComponents={busLive && !playing ? authorComponents : undefined}
          onDeleteComponent={deleteComponent}
          onNudgeTransform={busLive ? nudgeTransform : undefined}
        />
      )}
      {playing && effViewportSrc && engineStatus === "online" && !runPaused && (
        <div className="eui-play-frame" aria-hidden="true" />
      )}
      {effViewportSrc && engineStatus === "online" && !cameraHintDismissed && (
        <span
          style={{
            position: "absolute",
            top: "calc(var(--eui-top-inset, 12px) + 44px)",
            left: hideLeft ? 24 : 300,
            zIndex: 34,
            display: "inline-flex",
            alignItems: "center",
            gap: 8,
            padding: "4px 6px 4px 12px",
            borderRadius: 999,
            whiteSpace: "nowrap",
            background: "rgba(29, 28, 32, 0.92)",
            border: "1px solid rgba(255, 255, 255, 0.14)",
            boxShadow: "0 6px 18px rgba(0, 0, 0, 0.45)",
            color: "#c8c8d2",
            fontSize: 11.5,
            pointerEvents: "none",
          }}
        >
          middle-drag orbit / shift+middle pan / wheel zoom / double-click focus
          <button
            type="button"
            aria-label="Dismiss camera hint"
            onClick={dismissCameraHint}
            style={{
              pointerEvents: "auto",
              background: "none",
              border: "none",
              color: "#9a9aa4",
              cursor: "pointer",
              fontSize: 13,
              padding: "0 4px",
            }}
          >
            {"\u{2715}"}
          </button>
        </span>
      )}
      {dragAsset && effViewportSrc && (
        <div
          className="eui-drop-target"
          style={{
            position: "absolute",
            top: 96,
            bottom: 12,
            left: hideLeft ? 12 : 288,
            right: hideRight ? 12 : 344,
            zIndex: 30,
            border: "2px dashed rgba(94, 210, 255, 0.55)",
            borderRadius: 8,
            background: "rgba(20, 40, 55, 0.12)",
          }}
          onDragOver={(e) => {
            e.preventDefault();
            e.dataTransfer.dropEffect = "copy";
          }}
          onDrop={handleViewportDrop}
        />
      )}
      {playing && effViewportSrc && engineStatus === "online" && (
        <span
          className="eui-play-badge eui-play-badge--preview"
          role="status"
          title={"Edits made while the scene is running are temporary \u{2014} Stop restores the scene to its pre-play state."}
        >
          {playBadgeLabel(runPaused, sceneEmpty)}
        </span>
      )}
      {renderTuningOpen && effViewportSrc && (
        <DeRenderPanel
          onCommand={(line) => {
            const run = engineConsoleRunner(viewportRef.current);
            void run?.(line);
          }}
          onQueryState={() => {
            return engineConsoleRunner(viewportRef.current)?.("/renderstate") ?? Promise.reject();
          }}
          onClose={() => setRenderTuningOpen(false)}
          onOpenCameraSettings={live ? () => setCamSettingsOpen(true) : undefined}
        />
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
      {mcpConsent && (
        <McpPairingConsentModal host={mcpConsent.host} onAnswer={answerMcpConsent} />
      )}
      {assistantOpen && code?.project?.assistant && <Suspense fallback={<div role="status">Loading scene assistant&#x2026;</div>}>
        <SceneAssistantPanel selectedEntities={selectedIds.slice(0, 100).map(id => ({ id: String(id), name: findNodeName(effTree, id) || `Entity ${id}` }))} assistant={code.project.assistant} onPair={pairAssistant} onClose={() => setAssistantOpen(false)} />
      </Suspense>}
      {uiDesignerOpen && code?.project?.uiDesigner && <Suspense fallback={<div role="status">Loading UI Designer&#x2026;</div>}><DeUIDesigner project={code.project} onClose={() => setUIDesignerOpen(false)} /></Suspense>}
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
              Loading code editor&#x2026;
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
