import { lazy, Suspense, useEffect, useReducer, useRef, useState } from "react";
import { SdkProjectConnection, type SdkProject } from "@data/lib/fs/sdk-project";
import { emptySeed, seedFromCompositeJSON, buildViewportUrl } from "@data/lib/catalyst/creator-hub/scene-editor";
import type { CatalogItem } from "@data/lib/catalyst/creator-hub/asset-catalog.server";
import EditorWizard from "./EditorWizard";
import "./sdk-project.css";

const DevicePreviewPanel = lazy(() => import("./DevicePreviewPanel"));

type Loaded = { connection: SdkProjectConnection; project: SdkProject; composite?: string };
type ConnectionState =
  | { phase: "connecting"; url: string; request: number }
  | { phase: "ready"; url: string; request: number; loaded: Loaded }
  | { phase: "error"; url: string; request: number; error: string };
type ConnectionEvent =
  | { type: "start"; url: string; request: number }
  | { type: "ready"; url: string; request: number; loaded: Loaded }
  | { type: "error"; url: string; request: number; error: string };

export function sdkConnectionTransition(state: ConnectionState, event: ConnectionEvent): ConnectionState {
  if (event.type === "start") return { phase: "connecting", url: event.url, request: event.request };
  if (event.url !== state.url || event.request !== state.request || state.phase !== "connecting") return state;
  return event.type === "ready" ? { phase: "ready", url: event.url, request: event.request, loaded: event.loaded } : { phase: "error", url: event.url, request: event.request, error: event.error };
}

export default function SdkEditorWorkspace({ projectUrl, viewportSrc, catalog, onExit }: {
  projectUrl: string; viewportSrc: string; catalog?: CatalogItem[]; onExit: () => void;
}) {
  const request = useRef(0);
  const [connection, dispatch] = useReducer(sdkConnectionTransition, { phase: "connecting", url: projectUrl, request: 0 } as ConnectionState);
  const [attempt, setAttempt] = useState(0);
  const [devicePreview, setDevicePreview] = useState(false);
  const scopeChanged = connection.url !== projectUrl;
  useEffect(() => {
    const activeRequest = ++request.current;
    const controller = new AbortController();
    const active = () => request.current === activeRequest && !controller.signal.aborted;
    dispatch({ type: "start", url: projectUrl, request: activeRequest });
    setDevicePreview(false);
    void (async () => {
      const connection = new SdkProjectConnection(projectUrl);
      const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(15000)]);
      const project = await connection.connect(signal);
      if (!active()) return;
      const composite = project.compositePath ? await connection.read(project.compositePath) : undefined;
      if (!active()) return;
      if (composite) JSON.parse(composite);
      if (active()) dispatch({ type: "ready", url: projectUrl, request: activeRequest, loaded: { connection, project, composite } });
    })().catch((error: unknown) => { if (active()) dispatch({ type: "error", url: projectUrl, request: activeRequest, error: error instanceof Error ? error.message : String(error) }); });
    return () => { controller.abort(); };
  }, [projectUrl, attempt]);

  if (scopeChanged || connection.phase !== "ready") return <section className="editor-wizard" aria-label="SDK project">
    <h1>Open SDK project</h1>
    <p role={connection.phase === "error" && !scopeChanged ? "alert" : "status"}>{connection.phase === "error" && !scopeChanged ? connection.error : "Connecting to your project\u2026"}</p>
    {connection.phase === "error" && !scopeChanged && <button type="button" onClick={() => setAttempt((value) => value + 1)}>Retry connection</button>}
    <button type="button" onClick={onExit}>Back to Creator Hub</button>
  </section>;

  const { connection: projectConnection, project, composite } = connection.loaded;
  const metadata = project.scene as { scene?: { base?: string; parcels?: string[] } };
  const base = emptySeed(metadata.scene?.base || "0,0");
  base.scene.title = project.name;
  base.scene.base = metadata.scene?.base || "0,0";
  base.scene.parcels = metadata.scene?.parcels || [base.scene.base];
  const seed = composite ? seedFromCompositeJSON(JSON.parse(composite), base, []) : base;
  const original = new URL(viewportSrc, window.location.origin);
  const preview = {
    playUrl: original.pathname,
    realm: projectConnection.url,
    position: base.scene.base,
    preview: true,
    winitWorker: true,
    portables: original.searchParams.get("portables") || "",
  };
  return <><EditorWizard key={`${projectUrl}:${attempt}`} seed={seed} catalog={catalog}
      viewportSrc={buildViewportUrl({ ...preview, systemScene: original.searchParams.get("systemScene"), editorUi: true, editorSession: original.searchParams.get("editorSession") })}
      previewSrc={buildViewportUrl(preview)} rawComposite={composite}
      sdkProject={projectConnection} sdkDescriptor={project} onExit={onExit}
      onDevicePreview={() => setDevicePreview(true)}
      onPublish={() => window.location.assign(projectConnection.link("publish"))} />
    {devicePreview && <Suspense fallback={<p role="status">Loading device preview&#x2026;</p>}>
      <DevicePreviewPanel project={projectConnection} onClose={() => setDevicePreview(false)} />
    </Suspense>}
  </>;
}
