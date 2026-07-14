import { useCallback, useEffect, useMemo, useRef, useState, type ComponentProps } from "react";

import DeWorkspace from "@ui/editor/pages/DeWorkspace";
import { placedProjectContents } from "@ui/editor/project-cache";
import DeEditorAppBar, { DeEditorControlsBar as Controls } from "@ui/editor/components/DeEditorAppBar";
import { STARTER_TEMPLATES } from "@ui/creatorhub/pages/ChTemplates";
import { EDITOR_BUS_CHANNEL, type BusEnvelope, type PageToSceneMessage } from "@ui/generated/editor-bus";

import type {
  HierarchyNode,
  SceneEditorSeed,
} from "@data/lib/catalyst/creator-hub/scene-editor";
import type { CatalogItem } from "@data/lib/catalyst/creator-hub/asset-catalog.server";
import { buildScaffoldFiles } from "@data/lib/fs/scaffold-project";
import { hasTemplateComposite } from "@data/lib/fs/template-composites";
import {
  handleStore,
  slugifyProjectTitle,
  ensureHandlePermission,
} from "@data/lib/fs/handle-store";

export type EditorWizardProps = {
  seed: SceneEditorSeed;
  onExit?: () => void;
  onPublish?: (id?: string, draft?: string) => void;
  viewportSrc?: string;
  previewSrc?: string;
  rawComposite?: string;
  draftAssets?: Record<string, string>;
  catalog?: CatalogItem[];
  template?: string;
  failedToLoadLocal?: boolean;
};

type DiskSaveState =
  | { phase: "idle" }
  | { phase: "saving" }
  | { phase: "saved"; via: "fsa-handle" | "download"; entities: number; filename: string }
  | { phase: "canceled" }
  | { phase: "error"; message: string };

const MUTATING_TO_SCENE = new Set<PageToSceneMessage["type"]>([
  "add-entity",
  "set-component",
  "add-component",
  "delete-component",
  "entity-deleted",
  "component-written",
]);

// Hosts the editor: app bar, ribbon workspace, and the page-side plumbing the
// ribbon needs (disk save, publish draft, dirty tracking). All editing
// controls live in the workspace ribbon -- this page adds no chrome of its
// own beyond dismissible notices and a transient save-status strip.
export default function EditorWizard({
  seed,
  onExit,
  onPublish,
  viewportSrc,
  previewSrc,
  rawComposite,
  draftAssets,
  catalog,
  template,
  failedToLoadLocal,
}: EditorWizardProps) {
  const [templateNoticeDismissed, setTemplateNoticeDismissed] = useState(false);
  const [localErrorDismissed, setLocalErrorDismissed] = useState(false);
  const [liveCopyNoteDismissed, setLiveCopyNoteDismissed] = useState(false);
  const [diskSave, setDiskSave] = useState<DiskSaveState>({ phase: "idle" });
  const [enginePlaying, setEnginePlaying] = useState(false);
  const [engineStatus, setEngineStatus] = useState<"connecting" | "online" | "offline">(
    "connecting",
  );

  const dirtyRef = useRef(false);
  useEffect(() => {
    if (typeof window === "undefined" || typeof BroadcastChannel === "undefined") return undefined;
    const ch = new BroadcastChannel(EDITOR_BUS_CHANNEL);
    ch.onmessage = (ev) => {
      const env = (ev.data ?? null) as BusEnvelope | null;
      if (!env || typeof env !== "object" || !env.msg) return;
      if (env.to === "scene" && MUTATING_TO_SCENE.has(env.msg.type)) dirtyRef.current = true;
      if (env.to === "page" && env.msg.type === "drag-end") dirtyRef.current = true;
      if (env.to === "page" && env.msg.type === "play-state") {
        setEnginePlaying(env.msg.playing === true);
      }
    };
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (!dirtyRef.current) return;
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => {
      ch.close();
      window.removeEventListener("beforeunload", onBeforeUnload);
    };
  }, []);
  const guardedExit = onExit
    ? () => {
        if (
          dirtyRef.current &&
          typeof window !== "undefined" &&
          !window.confirm("You have unsaved changes. Leave the editor without saving?")
        ) {
          return;
        }
        onExit();
      }
    : undefined;

  const seedRef = useRef(seed);
  seedRef.current = seed;
  const draftAssetsRef = useRef(draftAssets);
  draftAssetsRef.current = draftAssets;
  const projectAssets = () => ({ ...draftAssetsRef.current, ...placedProjectContents() });

  const persistPublishDraft = async (): Promise<string | null> => {
    const scene = seedRef.current.scene;
    const slug = slugifyProjectTitle(scene.title);
    let composite: string | null = null;
    if (viewportSrc) {
      try {
        const [{ normalizeEngineComposite }, { createEditorBus }] = await Promise.all([
          import("@data/lib/fs/save-scene"),
          import("@ui/editor/editor-bus"),
        ]);
        const bus = createEditorBus();
        try {
          composite = normalizeEngineComposite(await bus.exportComposite(2000));
        } finally {
          bus.close();
        }
      } catch {
        composite = null;
      }
    }
    composite = composite ?? rawComposite ?? null;
    if (!composite) return null;
    await handleStore.putMeta(slug, {
      title: scene.title,
      base: scene.base,
      template: scene.template,
      composite,
      assets: projectAssets(),
    });
    return slug;
  };

  const guardedPublish = onPublish
    ? (id?: string) => {
        if (
          dirtyRef.current &&
          typeof window !== "undefined" &&
          !window.confirm(
            "You have unsaved changes. Continue to publish? A draft of this scene will be kept so you can come back.",
          )
        ) {
          return;
        }
        void persistPublishDraft()
          .catch(() => null)
          .then((draft) => onPublish(id, draft ?? undefined));
      }
    : undefined;

  const catalogItems = (catalog && catalog.length > 0
    ? catalog
    : seed.assetCatalog.models) as unknown as ComponentProps<typeof DeWorkspace>["catalog"];

  const tree = buildHierarchyTree(seed.hierarchy) as unknown as ComponentProps<
    typeof DeWorkspace
  >["tree"];
  const workspaceCode = useMemo(() => {
    const slug = slugifyProjectTitle(seed.scene.title);
    return {
      typesUrl: "/dcl-sdk-types.json",
      virtualFiles: buildScaffoldFiles({
        name: seed.scene.title,
        template: seed.scene.template,
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
  }, [seed.scene.title, seed.scene.base, seed.scene.template]);
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
  const prepareRealm = useCallback(async () => {
    if (!gameTemplateId) return;
    const { populateTemplateRealm } = await import("@data/lib/fs/project-realm");
    const res = await populateTemplateRealm({
      template: gameTemplateId,
      name: seedRef.current.scene.title,
      assets: { ...draftAssetsRef.current, ...placedProjectContents() },
    });
    if (!res.ok) {
      console.warn("[editor] template game realm population failed:", res.reason);
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

  async function onSaveToDisk() {
    if (diskSave.phase === "saving" || enginePlaying) return;
    setDiskSave({ phase: "saving" });
    try {
      const { saveSceneFromEngine } = await import("@data/lib/fs/save-scene");
      const res = await saveSceneFromEngine(
        seed.hierarchy,
        {},
        {
          project: {
            title: seed.scene.title,
            base: seed.scene.base,
            template: seed.scene.template,
            assets: projectAssets(),
          },
        },
      );
      if (!res.written) {
        setDiskSave({ phase: "canceled" });
      } else {
        dirtyRef.current = false;
        setDiskSave({
          phase: "saved",
          via: res.via === "download" ? "download" : "fsa-handle",
          entities: res.entities,
          filename: res.filename,
        });
      }
    } catch (e) {
      setDiskSave({
        phase: "error",
        message: e instanceof Error ? e.message : String(e ?? "unknown error"),
      });
    }
  }

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
          </span>
        ) : diskSave.phase === "canceled" ? (
          <span role="status" style={{ color: "var(--ink-7, rgba(255, 255, 255, 0.7))" }}>
            Save cancelled &#x2014; nothing was written to disk.
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
      <DeEditorAppBar
        title={seed.scene.title}
        viewportSrc={viewportSrc}
        previewSrc={previewSrc}
        engine={engineStatus}
        publishOptions={buildPublishOptions(seed.scene)}
        onExit={guardedExit}
        onPublish={guardedPublish}
      />

      <DeWorkspace
        title={seed.scene.title}
        tree={tree}
        inspector={inspector}
        catalog={catalogItems}
        local={localAssets}
        viewportSrc={viewportSrc}
        rawComposite={rawComposite}
        code={workspaceCode}
        prepareRealm={gameTemplateId ? prepareRealm : undefined}
        onEngineStatus={setEngineStatus}
        onSaveToDisk={enginePlaying ? undefined : onSaveToDisk}
        onPublish={guardedPublish ? () => guardedPublish() : undefined}
        saveState={
          diskSave.phase === "saving"
            ? "saving"
            : diskSave.phase === "error"
              ? "error"
              : diskSave.phase === "saved" && !dirtyRef.current
                ? "saved"
                : "idle"
        }
      />

      {localErrorBanner}
      {templateNotice}
      {liveCopyNote}
      {saveStatusStrip}
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
