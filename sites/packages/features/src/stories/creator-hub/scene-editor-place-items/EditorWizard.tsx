import { useCallback, useEffect, useMemo, useRef, useState, type ComponentProps } from "react";

import { useEditorPageMachine } from "@ui/editor/pages/useEditorPageMachine";
import { editorPageDirty, type EditorPageRequest } from "@ui/editor/page-machine";
import { INITIAL_WORKSPACE, type EditorWorkspaceSnapshot } from "@ui/editor/workspace-lifecycle";
import DeWorkspace from "@ui/editor/pages/DeWorkspace";
import type { DeWorkspaceCode } from "@ui/editor/types";
import DeSceneSettings from "@ui/editor/components/DeSceneSettings";
import { openSceneSettingsFile } from "@data/lib/fs/scene-settings-file";
import { runEditorEffect } from "@ui/editor/serial-effect";
import { createEditorBus, editorBusChannelFromViewportSrc } from "@ui/editor/editor-bus";
import { PROJECT_REALM_EFFECT, placedProjectContents } from "@ui/editor/project-cache";
import DeEditorAppBar, { DeEditorControlsBar as Controls } from "@ui/editor/components/DeEditorAppBar";
import { STARTER_TEMPLATES } from "@ui/creatorhub/pages/ChTemplates";
import { type BusEnvelope, type PageToSceneMessage } from "@ui/generated/editor-bus";

import type {
  HierarchyNode,
  SceneEditorSeed,
} from "@data/lib/catalyst/creator-hub/scene-editor";
import type { CatalogItem } from "@data/lib/catalyst/creator-hub/asset-catalog.server";
import { buildScaffoldFiles } from "@data/lib/fs/scaffold-project";
import { hasTemplateComposite } from "@data/lib/fs/template-composites";
import type { SdkProjectConnection, SdkProject } from "@data/lib/fs/sdk-project";
import {
  NO_COMPOSITE_CODE_ONLY_HINT,
  NO_COMPOSITE_HINT,
  OPEN_FAILED_HINT,
} from "@data/lib/fs/local-scene";
import {
  handleStore,
  slugifyProjectTitle,
  ensureHandlePermission,
} from "@data/lib/fs/handle-store";

function sdkFileProject(connection: SdkProjectConnection): NonNullable<DeWorkspaceCode["project"]> {
  return {
    id: connection.url,
    list: () => connection.list(),
    read: path => connection.read(path),
    readOnly: path => connection.readOnly(path),
    write: (path, content) => connection.write(path, content),
    remove: path => connection.remove(path),
    createFileSession: () => sdkFileProject(connection.createFileSession()),
    assistant: connection.assistant,
    assets: connection.assets,
    uiDesigner: connection.uiDesigner,
  };
}

type EditorWizardProps = {
  seed: SceneEditorSeed;
  projectSlug?: string;
  onExit?: () => void;
  onPublish?: (id?: string, draft?: string) => void;
  viewportSrc?: string;
  previewSrc?: string;
  rawComposite?: string;
  draftAssets?: Record<string, string>;
  catalog?: CatalogItem[];
  template?: string;
  failedToLoadLocal?: boolean;
  from?: string | null;
  sdkProject?: SdkProjectConnection;
  sdkDescriptor?: SdkProject;
  onDevicePreview?: () => void;
};

type DiskSaveState =
  | { phase: "idle" }
  | { phase: "saving" }
  | {
      phase: "saved";
      via: "fsa-handle" | "download";
      entities: number;
      filename: string;
      serverSynced?: boolean;
    }
  | { phase: "canceled"; serverSynced?: boolean }
  | { phase: "error"; message: string };

type DiskOpenState =
  | { phase: "idle" }
  | { phase: "picking" | "reading" | "opening" }
  | { phase: "error"; message: string };

const MUTATING_TO_SCENE = new Set<PageToSceneMessage["type"]>([
  "add-entity",
  "set-component",
  "add-component",
  "delete-component",
  "entity-deleted",
  "component-written",
]);

export default function EditorWizard(props: EditorWizardProps) {
  return <EditorWizardSession key={`${props.projectSlug || props.seed.scene.title}:${props.viewportSrc ?? ""}:${props.sdkProject?.url ?? ""}`} {...props} />;
}

function EditorWizardSession({
  sdkProject,
  sdkDescriptor,
  onDevicePreview,
  seed,
  projectSlug,
  onExit,
  onPublish,
  viewportSrc,
  previewSrc,
  rawComposite,
  draftAssets,
  catalog,
  template,
  failedToLoadLocal,
  from = null,
}: EditorWizardProps) {
  const [templateNoticeDismissed, setTemplateNoticeDismissed] = useState(false);
  const [localErrorDismissed, setLocalErrorDismissed] = useState(false);
  const [liveCopyNoteDismissed, setLiveCopyNoteDismissed] = useState(false);
  const [diskSave, setDiskSave] = useState<DiskSaveState>({ phase: "idle" });
  const [diskOpen, setDiskOpen] = useState<DiskOpenState>({ phase: "idle" });
  const projectKey = projectSlug || slugifyProjectTitle(seed.scene.title);
  const operations = useEditorPageMachine(`${projectKey}:${viewportSrc ?? ""}:${sdkProject?.url ?? ""}`);
  const [workspace, setWorkspace] = useState<EditorWorkspaceSnapshot>(INITIAL_WORKSPACE);
  const activeViewportSrc = workspace.destination ?? viewportSrc;
  const enginePlaying = workspace.playing;
  const operationsBusy = !!operations.state.pending;
  const canOperate = workspace.capabilities.open && !operationsBusy;
  const canSave = canOperate && (!viewportSrc || workspace.capabilities.save);
  const finish = (request: EditorPageRequest, status: "completed" | "cancelled" | "failed", saved = false, error?: string) =>
    operations.send({ type: "finished", request, status, saved, error });
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [engineStatus, setEngineStatus] = useState<"connecting" | "online" | "offline">(
    "connecting",
  );

  const isDirty = () => editorPageDirty(operations.current.current);
  const workspaceRef = useRef(workspace);
  const onLifecycle = useCallback((snapshot: EditorWorkspaceSnapshot) => {
    const previous = workspaceRef.current;
    if (snapshot.session !== previous.session || snapshot.destination !== previous.destination || (previous.ready && !snapshot.ready)) {
      const pending = operations.current.current.pending;
      operations.send({ type: "connection-changed" });
      if (pending) {
        const message = "The editor reconnected. Check your scene and try again.";
        if (pending.command === "open") setDiskOpen({ phase: "error", message });
        else setDiskSave({ phase: "error", message });
      }
    }
    workspaceRef.current = snapshot;
    setWorkspace(snapshot);
  }, [operations.send]);
  useEffect(() => {
    if (typeof window === "undefined") return undefined;
    const channel = editorBusChannelFromViewportSrc(activeViewportSrc);
    const ch = !channel || typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel(channel);
    if (ch) ch.onmessage = (ev) => {
      const env = (ev.data ?? null) as BusEnvelope | null;
      if (!env || typeof env !== "object" || !env.msg) return;
      if (env.to === "scene" && MUTATING_TO_SCENE.has(env.msg.type)) operations.send({ type: "edited" });
      if (env.to === "scene" && env.msg.type === "rpc" && ["moveHierarchy", "pasteEntities", "pasteComponent", "writeComponents", "removeComponent", "removeEntities", "addEntity"].includes(env.msg.method)) operations.send({ type: "edited" });
      if (env.to === "page" && env.msg.type === "drag-end") operations.send({ type: "edited" });
    };
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (!isDirty() && !operations.current.current.pending) return;
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => {
      ch?.close();
      window.removeEventListener("beforeunload", onBeforeUnload);
    };
  }, [activeViewportSrc]);
  const guardedExit = onExit
    ? () => {
        if (operations.current.current.pending) return;
        if (
          isDirty() &&
          typeof window !== "undefined" &&
          !window.confirm("You have unsaved changes. Leave the editor without saving?")
        ) {
          return;
        }
        onExit();
      }
    : undefined;

  const sceneTitleRef = useRef(seed.scene.title);
  const onSceneNameChange = useCallback((name: string) => { if (sceneTitleRef.current !== name) operations.send({ type: "edited" }); sceneTitleRef.current = name; }, [operations.send]);
  const seedRef = useRef(seed);
  seedRef.current = seed;
  const draftAssetsRef = useRef(draftAssets);
  draftAssetsRef.current = draftAssets;
  const projectAssets = () => ({ ...draftAssetsRef.current, ...placedProjectContents() });

  const exportComposite = async (request: EditorPageRequest) => {
    const signal = operations.signal(request);
    signal.throwIfAborted();
    const bus = createEditorBus(activeViewportSrc);
    const close = () => bus.close();
    signal.addEventListener("abort", close, { once: true });
    try { return await bus.exportComposite(); }
    finally { signal.removeEventListener("abort", close); close(); }
  };
  const persistenceKey = `editor-project:${sdkProject?.url ?? projectKey}`;
  const persistPublishDraft = (request: EditorPageRequest): Promise<string | null> =>
    runEditorEffect(persistenceKey, () => operations.accepts(request), () => writePublishDraft(request));
  const writePublishDraft = async (request: EditorPageRequest): Promise<string | null> => {
    const check = () => { if (!operations.accepts(request)) throw new Error("The editor session changed. Try again."); };
    check();
    const scene = { ...seedRef.current.scene, title: sceneTitleRef.current };
    const slug = projectKey;
    let composite: string | null = null;
    if (viewportSrc) {
      const { requireEngineComposite } = await import("@data/lib/fs/save-scene");
      check();
      composite = await requireEngineComposite(12000, () => exportComposite(request));
      check();
    }
    composite = composite ?? rawComposite ?? null;
    if (!composite) throw new Error("The scene could not be captured. Reconnect and try again.");
    if (sdkProject) {
      await sdkProject.write(sdkDescriptor?.compositePath || "main.composite", composite, operations.signal(request));
      check();
      return null;
    }
    await handleStore.putMeta(slug, {
      title: scene.title,
      base: scene.base,
      template: scene.template,
      composite,
      assets: projectAssets(),
    }, operations.signal(request));
    check();
    try {
      const { pushServerDraft } = await import(
        "@data/lib/catalyst/creator-hub/scene-drafts-client"
      );
      check();
      await pushServerDraft(slug, {
        composite,
        title: scene.title,
        base: scene.base,
        ...(scene.template ? { template: scene.template } : {}),
        assets: projectAssets(),
      }, operations.signal(request));
    } catch {
    }
    check();
    return slug;
  };

  const guardedPublish = onPublish
    ? (id?: string) => {
        if (!canSave) return;
        if (
          !sdkProject && isDirty() &&
          typeof window !== "undefined" &&
          !window.confirm(
            "You have unsaved changes. Continue to publish? A draft of this scene will be kept so you can come back.",
          )
        ) {
          return;
        }
        const request = operations.begin("publish", canSave);
        if (!request) return;
        void persistPublishDraft(request)
          .then((draft) => {
            if (!operations.accepts(request)) return;
            finish(request, "completed", true);
            onPublish(id, draft ?? undefined);
          })
          .catch((error) => {
            if (!operations.accepts(request)) return;
            const message = error instanceof Error ? error.message : "The scene could not be prepared for publishing.";
            finish(request, "failed", false, message);
            setDiskSave({ phase: "error", message });
          });
      }
    : undefined;

  const catalogItems = (catalog && catalog.length > 0
    ? catalog
    : seed.assetCatalog.models) as unknown as ComponentProps<typeof DeWorkspace>["catalog"];

  const tree = buildHierarchyTree(seed.hierarchy) as unknown as ComponentProps<
    typeof DeWorkspace
  >["tree"];
  const workspaceCode = useMemo(() => {
    if (sdkProject) return {
      typesUrl: "/dcl-sdk-types.json",
      project: sdkFileProject(sdkProject),
    };
    const slug = projectKey;
    return {
      typesUrl: "/dcl-sdk-types.json",
      virtualFiles: buildScaffoldFiles({
        name: seed.scene.title,
        template: seed.scene.template,
        parcels: seed.scene.parcels,
      }),
      getDir: async () => {
        try {
          const h = await handleStore.get(slug);
          if (!h) return null;
          return (await ensureHandlePermission(h, "readwrite")) ? h : null;
        } catch {
          return null;
        }
      },
      hydrate: async () => {
        try {
          const meta = await handleStore.getMeta(slug);
          return meta?.codeFiles ?? null;
        } catch {
          return null;
        }
      },
      persist: async (path: string, text: string) => {
        await handleStore.putMeta(slug, {
          title: seed.scene.title,
          base: seed.scene.base,
          ...(seed.scene.template ? { template: seed.scene.template } : {}),
          codeFiles: { [path]: text },
        });
      },
    };
  }, [projectKey, seed.scene.title, seed.scene.base, seed.scene.template, sdkProject]);
  const loadSettings = useCallback(() => openSceneSettingsFile(workspaceCode), [workspaceCode]);
  const localAssets =
    (seed.assetCatalog as { local?: { path: string; folder: string }[] }).local ?? [];
  const selectedNode = seed.hierarchy.find((n) => n.selected) ?? seed.hierarchy[0];
  const inspector = selectedNode
    ? {
        name: selectedNode.name,
        id: String(selectedNode.entity),
        components: selectedNode.components,
        transform: seed.transformIdentity,
      }
    : undefined;

  const templateId = template ?? seed.scene.template;
  const starter = templateId
    ? STARTER_TEMPLATES.find((t) => t.id === templateId)
    : undefined;

  const gameTemplateId = templateId && hasTemplateComposite(templateId) ? templateId : null;
  const prepareRealm = useCallback(async (signal?: AbortSignal) => {
    if (!gameTemplateId) return;
    const { populateTemplateRealm } = await import("@data/lib/fs/project-realm");
    const res = await populateTemplateRealm({
      signal,
      template: gameTemplateId,
      name: seedRef.current.scene.title,
      assets: { ...draftAssetsRef.current, ...placedProjectContents() },
    });
    if (!res.ok) {
      throw new Error(`Could not prepare the template realm: ${res.reason}`);
    }
  }, [gameTemplateId]);

  const templateNotice =
    starter && !templateNoticeDismissed ? (
      <div
        className="editor-wizard__note"
        role="note"
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "12px",
          margin: "0 0 8px",
          padding: "8px 12px",
          fontSize: "13px",
          lineHeight: 1.5,
          borderRadius: "8px",
          background: "var(--ink-1, rgba(255, 255, 255, 0.06))",
          color: "var(--ink-8, rgba(255, 255, 255, 0.8))",
        }}
      >
        <span>
          This scene started from the <strong>{starter.title}</strong> template &#x2014;
          its entities are yours to edit or delete. Press <strong>Play</strong> to
          run the template&rsquo;s starter game (Stop restores your scene). The
          Code panel shows the same starter code, but edits there don&rsquo;t
          change what Play runs yet &#x2014; run edited code with <code>npm start</code>{" "}
          after saving the project to disk.
          {starter.github_link ? (
            <>
              {" \u{B7} "}
              <a
                href={starter.github_link}
                target="_blank"
                rel="noreferrer"
                style={{ color: "inherit", textDecoration: "underline" }}
              >
                View the original scene on GitHub
              </a>
            </>
          ) : null}
        </span>
        <button
          type="button"
          className="editor-wizard__btn"
          aria-label="Dismiss template notice"
          onClick={() => setTemplateNoticeDismissed(true)}
        >
          Dismiss
        </button>
      </div>
    ) : null;

  const liveCopyNote =
    seed.scene.live && !liveCopyNoteDismissed ? (
      <div
        className="editor-wizard__note"
        role="note"
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "12px",
          margin: "0 0 8px",
          padding: "8px 12px",
          fontSize: "13px",
          lineHeight: 1.5,
          borderRadius: "8px",
          background: "var(--ink-1, rgba(255, 255, 255, 0.06))",
          color: "var(--ink-8, rgba(255, 255, 255, 0.8))",
        }}
      >
        <span>
          You are editing your deployed copy &#x2014; changes stay in this browser
          until saved.
        </span>
        <button
          type="button"
          className="editor-wizard__btn"
          aria-label="Dismiss"
          onClick={() => setLiveCopyNoteDismissed(true)}
        >
          Dismiss
        </button>
      </div>
    ) : null;

  const localErrorBanner =
    failedToLoadLocal && !localErrorDismissed ? (
      <div
        className="editor-wizard__banner"
        role="alert"
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "12px",
          margin: "0 0 8px",
          padding: "8px 12px",
          fontSize: "13px",
          lineHeight: 1.5,
          borderRadius: "8px",
          background: "var(--error-bg, rgba(255, 92, 92, 0.14))",
          color: "var(--error, #ff8080)",
        }}
      >
        <span>
          We couldn&rsquo;t load that scene from your computer, so this is a fresh
          empty scene. Reopen the project, or reload to try again.
        </span>
        <span style={{ display: "flex", gap: "8px", flexShrink: 0 }}>
          <button
            type="button"
            className="editor-wizard__btn editor-wizard__btn--primary"
            onClick={() => {
              if (typeof window !== "undefined") window.location.reload();
            }}
          >
            Reload
          </button>
          <button
            type="button"
            className="editor-wizard__btn"
            aria-label="Dismiss"
            onClick={() => setLocalErrorDismissed(true)}
          >
            Dismiss
          </button>
        </span>
      </div>
    ) : null;

  async function onOpenFromDisk() {
    if (!canOperate) return;
    if (
      isDirty() &&
      typeof window !== "undefined" &&
      !window.confirm("You have unsaved changes. Open another scene and lose them?")
    ) {
      return;
    }
    const request = operations.begin("open", canOperate);
    if (!request) return;
    setDiskOpen({ phase: "picking" });
    try {
      const { openLocalScene, stageLocalSceneForEditor } = await import(
        "@data/lib/fs/local-scene"
      );
      if (!operations.accepts(request)) return;
      const out = await openLocalScene({ onPhase: (phase) => { if (operations.accepts(request)) setDiskOpen({ phase }); } });
      if (!operations.accepts(request)) return;
      if (out.status === "cancelled") {
        finish(request, "cancelled");
        setDiskOpen({ phase: "idle" });
        return;
      }
      if (out.status === "no-composite") {
        finish(request, "failed");
        setDiskOpen({
          phase: "error",
          message: out.hasSceneJson ? NO_COMPOSITE_CODE_ONLY_HINT : NO_COMPOSITE_HINT,
        });
        return;
      }
      setDiskOpen({ phase: "opening" });
      await runEditorEffect(PROJECT_REALM_EFFECT, () => operations.accepts(request), () => stageLocalSceneForEditor(out.result, operations.signal(request)));
      if (!operations.accepts(request)) return;
      finish(request, "completed", true);
      if (typeof window !== "undefined") {
        window.location.assign(
          `/creator-hub/scene-editor?source=local&from=${encodeURIComponent(from ?? "editor")}`,
        );
      }
    } catch {
      if (!operations.accepts(request)) return;
      finish(request, "failed", false, OPEN_FAILED_HINT);
      setDiskOpen({ phase: "error", message: OPEN_FAILED_HINT });
    }
  }

  async function onSaveToDisk() {
    const request = operations.begin("save", canSave);
    if (!request) return;
    if (sdkProject) {
      setDiskSave({ phase: "saving" });
      try {
        await persistPublishDraft(request);
        if (!operations.accepts(request)) return;
        finish(request, "completed", true);
        setDiskSave({ phase: "saved", via: "fsa-handle", filename: sdkDescriptor?.compositePath || "main.composite", entities: seed.hierarchy.length });
      } catch (error) {
        if (!operations.accepts(request)) return;
        finish(request, "failed");
        setDiskSave({ phase: "error", message: error instanceof Error ? error.message : String(error) });
      }
      return;
    }
    setDiskSave({ phase: "saving" });
    try {
      const { saveSceneFromEngine } = await import("@data/lib/fs/save-scene");
      if (!operations.accepts(request)) return;
      const res = await runEditorEffect(persistenceKey, () => operations.accepts(request), () => saveSceneFromEngine(
        seed.hierarchy,
        {},
        {
          signal: operations.signal(request),
          exportComposite: () => exportComposite(request),
          project: {
            slug: projectKey,
            title: sceneTitleRef.current,
            base: seed.scene.base,
            template: seed.scene.template,
            assets: projectAssets(),
          },
        },
      ));
      if (!operations.accepts(request)) return;
      if (!res.written) {
        finish(request, "cancelled", res.serverSynced === true);
        setDiskSave({ phase: "canceled", serverSynced: res.serverSynced });
      } else {
        finish(request, "completed", true);
        setDiskSave({
          phase: "saved",
          via: res.via === "download" ? "download" : "fsa-handle",
          entities: res.entities,
          filename: res.filename,
          serverSynced: res.serverSynced,
        });
      }
    } catch (e) {
      if (!operations.accepts(request)) return;
      finish(request, "failed");
      setDiskSave({
        phase: "error",
        message: e instanceof Error ? e.message : String(e ?? "unknown error"),
      });
    }
  }

  const openStatusStrip =
    diskOpen.phase === "idle" ? null : (
      <Controls label="Open">
        {diskOpen.phase === "error" ? (
          <span role="alert" style={{ color: "var(--error, #ff8080)" }}>
            {diskOpen.message}
          </span>
        ) : (
          <span className="editor-wizard__spinner" role="status">
            {diskOpen.phase === "picking"
              ? "Pick the project folder that holds your saved scene\u{2026}"
              : diskOpen.phase === "reading"
                ? "Reading the project folder\u{2026}"
                : "Opening the scene\u{2026}"}
          </span>
        )}
        {diskOpen.phase === "error" && (
          <button
            type="button"
            className="editor-wizard__btn"
            onClick={() => setDiskOpen({ phase: "idle" })}
          >
            Dismiss
          </button>
        )}
      </Controls>
    );

  const saveStatusStrip =
    diskSave.phase === "idle" ? null : (
      <Controls label="Save">
        {diskSave.phase === "saving" ? (
          <span className="editor-wizard__spinner" role="status">
            Saving your scene to disk&#x2026;
          </span>
        ) : diskSave.phase === "saved" ? (
          <span className="editor-wizard__saved" role="status">
            {diskSave.via === "download"
              ? "Scene composite downloaded \u{2014} re-save it over your project's main.composite."
              : `Wrote ${diskSave.filename} to disk (${diskSave.entities} entities).`}
            {diskSave.serverSynced === true
              ? " Also saved to your account."
              : diskSave.serverSynced === false
                ? " Cloud copy not saved \u{2014} sign in to keep scenes across browsers."
                : null}
          </span>
        ) : diskSave.phase === "canceled" ? (
          <span role="status" style={{ color: "var(--ink-7, rgba(255, 255, 255, 0.7))" }}>
            {diskSave.serverSynced === true
              ? "No local folder chosen \u{2014} but the scene was saved to your account."
              : "Save cancelled \u{2014} nothing was written."}
          </span>
        ) : (
          <span role="alert" style={{ color: "var(--error, #ff8080)" }}>
            Save failed: {diskSave.message}
          </span>
        )}
        {diskSave.phase !== "saving" && (
          <button
            type="button"
            className="editor-wizard__btn"
            onClick={() => setDiskSave({ phase: "idle" })}
          >
            Dismiss
          </button>
        )}
      </Controls>
    );

  return (
    <div className="editor-wizard">
      <DeWorkspace
        onLifecycle={onLifecycle}
        operationsBusy={operationsBusy}
        renderHeader={(navigation, scene) => (
          <DeEditorAppBar
            exitDisabled={operationsBusy}
            busy={enginePlaying || workspace.busy || operationsBusy}
            title={scene.name}
            onRename={scene.rename}
            projectTools={<nav aria-label="Project tools" className="editor-wizard__project-tools">
              <button type="button" className="editor-wizard__btn" disabled={!canOperate} onClick={() => setSettingsOpen(true)}>Scene settings</button>
              {sdkProject && <a href={sdkProject.link("storage")} target="_blank" rel="noreferrer">Storage</a>}
              {onDevicePreview && <button type="button" className="editor-wizard__btn" onClick={onDevicePreview}>Device preview</button>}
              {sdkProject && <span title={sdkDescriptor?.capabilities.watch ? "Preview rebuilds on save" : "File watching is off"}>SDK project</span>}
            </nav>}
            viewportSrc={viewportSrc}
            previewSrc={previewSrc}
            engine={engineStatus}
            publishOptions={buildPublishOptions(seed.scene)}
            onExit={guardedExit}
            onPublish={canSave ? guardedPublish : undefined}
          >{navigation}</DeEditorAppBar>
        )}
        title={seed.scene.title}
        onSceneNameChange={onSceneNameChange}
        tree={tree}
        inspector={inspector}
        catalog={catalogItems}
        local={localAssets}
        viewportSrc={viewportSrc}
        rawComposite={rawComposite}
        code={workspaceCode}
        prepareRealm={!sdkProject && gameTemplateId ? prepareRealm : undefined}
        onEngineStatus={setEngineStatus}
        onSaveToDisk={canSave ? onSaveToDisk : undefined}
        onSceneSettings={!canOperate ? undefined : () => setSettingsOpen(true)}
        onOpenFromDisk={!canOperate || sdkProject ? undefined : () => void onOpenFromDisk()}
        onPublish={canSave && guardedPublish ? () => guardedPublish() : undefined}
        sceneInfo={{
          base: seed.scene.base,
          parcels: seed.scene.parcels,
          template: seed.scene.template ?? null,
        }}
        saveState={
          diskSave.phase === "saving"
            ? "saving"
            : diskSave.phase === "error"
              ? "error"
              : (diskSave.phase === "saved" ||
                    (diskSave.phase === "canceled" && diskSave.serverSynced === true)) &&
                  !isDirty()
                ? "saved"
                : "idle"
        }
      />

      {localErrorBanner}
      {templateNotice}
      {liveCopyNote}
      {operations.state.pending?.command === "publish" && <Controls label="Publish"><span role="status">{"Preparing your scene for publishing\u2026"}</span></Controls>}
      {openStatusStrip}
      {saveStatusStrip}
      {settingsOpen && <DeSceneSettings load={loadSettings} onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}

type TreeNode = {
  id: string;
  name: string;
  selected: boolean;
  expanded: boolean;
  children: TreeNode[];
};

function buildHierarchyTree(nodes: HierarchyNode[]): TreeNode[] {
  const byId = new Map<number, TreeNode>();
  for (const n of nodes) {
    byId.set(n.entity, {
      id: String(n.entity),
      name: n.name,
      selected: n.selected,
      expanded: false,
      children: [],
    });
  }
  const roots: TreeNode[] = [];
  for (const n of nodes) {
    const node = byId.get(n.entity)!;
    const parent = n.parent !== n.entity ? byId.get(n.parent) : undefined;
    if (parent) {
      parent.children.push(node);
      parent.expanded = true;
    } else {
      roots.push(node);
    }
  }
  return roots;
}

type PublishOption = { id: string; label: string };

function buildPublishOptions(scene: SceneEditorSeed["scene"]): PublishOption[] {
  const options: PublishOption[] = [{ id: "publish-scene", label: "Publish Scene" }];
  if (scene.live) {
    const isCoords = /^-?\d+\s*,\s*-?\d+$/.test(scene.pointer);
    const destination = !isCoords && scene.pointer ? scene.pointer : scene.base;
    if (destination) {
      options.push({ id: "republish", label: `Republish to ${destination}` });
    }
  }
  return options;
}
