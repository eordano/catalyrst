import { lazy, Suspense, useEffect, useState } from "react";
import { SdkProjectConnection, type SdkProject } from "@data/lib/fs/sdk-project";
import { emptySeed, seedFromCompositeJSON, buildViewportUrl } from "@data/lib/catalyst/creator-hub/scene-editor";
import type { CatalogItem } from "@data/lib/catalyst/creator-hub/asset-catalog.server";
import EditorWizard from "./EditorWizard";
import "./sdk-project.css";

const DevicePreviewPanel = lazy(() => import("./DevicePreviewPanel"));

type Loaded = { connection: SdkProjectConnection; project: SdkProject; composite?: string };

export default function SdkEditorWorkspace({ projectUrl, viewportSrc, catalog, onExit }: {
  projectUrl: string; viewportSrc: string; catalog?: CatalogItem[]; onExit: () => void;
}) {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [devicePreview, setDevicePreview] = useState(false);
  useEffect(() => {
    let disposed = false;
    setLoaded(null);
    setDevicePreview(false);
    setError(null);
    void (async () => {
      const connection = new SdkProjectConnection(projectUrl);
      const project = await connection.connect();
      const composite = project.compositePath ? await connection.read(project.compositePath) : undefined;
      if (composite) JSON.parse(composite);
      if (!disposed) setLoaded({ connection, project, composite });
    })().catch((error: unknown) => { if (!disposed) setError(error instanceof Error ? error.message : String(error)); });
    return () => { disposed = true; };
  }, [projectUrl, attempt]);

  if (!loaded) return <section className="editor-wizard" aria-label="SDK project">
    <h1>Open SDK project</h1>
    <p role={error ? "alert" : "status"}>{error || "Connecting to your project\u2026"}</p>
    {error && <button type="button" onClick={() => setAttempt((value) => value + 1)}>Retry connection</button>}
    <button type="button" onClick={onExit}>Back to Creator Hub</button>
  </section>;

  const { connection, project, composite } = loaded;
  const metadata = project.scene as { scene?: { base?: string; parcels?: string[] } };
  const base = emptySeed(metadata.scene?.base || "0,0");
  base.scene.title = project.name;
  base.scene.base = metadata.scene?.base || "0,0";
  base.scene.parcels = metadata.scene?.parcels || [base.scene.base];
  const seed = composite ? seedFromCompositeJSON(JSON.parse(composite), base, []) : base;
  const original = new URL(viewportSrc, window.location.origin);
  const preview = {
    playUrl: original.pathname,
    realm: connection.url,
    position: base.scene.base,
    preview: true,
    portables: original.searchParams.get("portables") || "",
  };
  return <><EditorWizard key={`${projectUrl}:${attempt}`} seed={seed} catalog={catalog}
      viewportSrc={buildViewportUrl({ ...preview, systemScene: original.searchParams.get("systemScene"), editorUi: true })}
      previewSrc={buildViewportUrl(preview)} rawComposite={composite}
      sdkProject={connection} sdkDescriptor={project} onExit={onExit}
      onDevicePreview={() => setDevicePreview(true)}
      onPublish={() => window.location.assign(connection.link("publish"))} />
    {devicePreview && <Suspense fallback={<p role="status">Loading device preview&#x2026;</p>}>
      <DevicePreviewPanel project={connection} onClose={() => setDevicePreview(false)} />
    </Suspense>}
  </>;
}
