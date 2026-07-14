import { useCallback, useEffect, useMemo, useRef, useState, type ComponentProps } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import DeWorkspace from "@ui/editor/pages/DeWorkspace";
import { placedProjectContents } from "@ui/editor/project-cache";
import { DeNewEntityModal } from "@ui/editor/components/DeNewEntityDialog";
import DeEditorAppBar, { DeEditorControlsBar as Controls } from "@ui/editor/components/DeEditorAppBar";
import { STARTER_TEMPLATES } from "@ui/creatorhub/pages/ChTemplates";
import { EDITOR_BUS_CHANNEL, type BusEnvelope, type PageToSceneMessage } from "@ui/generated/editor-bus";

import type { TrackContext } from "@core/lib/telemetry/track";
import type {
  Asset,
  ComponentDef,
  HierarchyNode,
  SceneEditorSeed,
} from "@data/lib/catalyst/creator-hub/scene-editor";
import type { CatalogItem } from "@data/lib/catalyst/creator-hub/asset-catalog.server";
import {
  editorMachine,
  resolveEditorSnapshot,
  simulateSave,
  slugToState,
  stateToSlug,
  type SaveFn,
  type TrackFn,
} from "./machine";
import { buildScaffoldFiles } from "@data/lib/fs/scaffold-project";
import { hasTemplateComposite } from "@data/lib/fs/template-composites";
import {
  handleStore,
  slugifyProjectTitle,
  ensureHandlePermission,
} from "@data/lib/fs/handle-store";

export type EditorWizardProps = {
  trackCtx: TrackContext;
  seed: SceneEditorSeed;
  initialStep?: string;
  save?: SaveFn;
  track?: TrackFn;
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

export default function EditorWizard({
  trackCtx,
  seed,
  initialStep,
  save,
  track,
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
  const [searchParams] = useSearchParams();
  const [gen, setGen] = useState(0);

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <EditorWizardInner
      key={gen}
      stateId={stateId}
      onRestart={() => setGen((g) => g + 1)}
      trackCtx={trackCtx}
      seed={seed}
      save={save}
      track={track}
      onExit={onExit}
      onPublish={onPublish}
      viewportSrc={viewportSrc}
      previewSrc={previewSrc}
      rawComposite={rawComposite}
      draftAssets={draftAssets}
      catalog={catalog}
      template={template}
      failedToLoadLocal={failedToLoadLocal}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  onRestart: () => void;
  trackCtx: TrackContext;
  seed: SceneEditorSeed;
  save?: SaveFn;
  track?: TrackFn;
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

function EditorWizardInner({ stateId, onRestart, trackCtx, seed, save, track, onExit, onPublish, viewportSrc, previewSrc, rawComposite, draftAssets, catalog, template, failedToLoadLocal }: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const [templateNoticeDismissed, setTemplateNoticeDismissed] = useState(false);
  const [localErrorDismissed, setLocalErrorDismissed] = useState(false);
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
  const realSave = useRef<SaveFn>(async (args) => {
    const { saveSceneFromEngine } = await import("@data/lib/fs/save-scene");
    const res = await saveSceneFromEngine(
      seedRef.current.hierarchy,
      {
        placed: args.placed
          ? {
              entity: args.placed.entity,
              assetId: args.placed.assetId,
              assetName: args.placed.assetName,
            }
          : undefined,
        selected: args.selected,
        modifiedName: args.modifiedName,
        component: args.component,
        deleted: args.deleted,
      },
      {
        project: {
          title: seedRef.current.scene.title,
          base: seedRef.current.scene.base,
          template: seedRef.current.scene.template,
          assets: projectAssets(),
        },
      },
    );
    if (!res.written) {
      throw new Error("Save cancelled — nothing was written to disk.");
    }
    return { composite: res.filename, entities: res.entities, via: res.via };
  }).current;

  const effectiveSave = save ?? realSave;

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

  const snapshot = useRef(
    resolveEditorSnapshot({
      step: stateId,
      trackCtx,
      save: effectiveSave,
      track,
    }),
  ).current;

  const [state, send] = useMachine(editorMachine, {
    input: { trackCtx, save: effectiveSave, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  const placedName = state.context.placed?.assetName ?? "Entity";
  const placingName =
    state.context.pendingAsset?.name ?? state.context.placed?.assetName ?? "Entity";

  const previewSave =
    state.context.result?.simulated ?? state.context.save === simulateSave;

  const savedDetail =
    state.context.result?.via === "download"
      ? "Scene composite downloaded — re-save it over your project's main.composite."
      : state.context.result?.via === "fsa-handle"
        ? `Wrote main.composite to disk (${state.context.result?.entities ?? 0} entities).`
        : "All changes saved.";

  const assetQuery = (state.context.query ?? "").trim().toLowerCase();
  const modelPool: Asset[] =
    catalog && catalog.length > 0
      ? catalog.map((c) => ({
          id: c.id,
          name: c.name,
          pack: c.pack,
          src: c.glbFile,
          hue: 210,
        }))
      : seed.assetCatalog.models;
  const guidedModels: Asset[] = assetQuery
    ? modelPool.filter((m) => `${m.name} ${m.pack ?? ""}`.toLowerCase().includes(assetQuery))
    : modelPool;

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
  const inspector = state.context.placed
    ? {
        name: state.context.placed.assetName,
        id: String(state.context.placed.entity),
        components: [] as string[],
        transform: seed.transformIdentity,
      }
    : selectedNode
      ? {
          name: selectedNode.name,
          id: String(selectedNode.entity),
          components: selectedNode.components,
          transform: seed.transformIdentity,
        }
      : undefined;

  const modifiable = seed.hierarchy.filter((n) => n.entity !== 0);
  const selected = state.context.selected;
  const modifyName = state.context.modifiedName ?? selected?.name ?? "Entity";
  const modifyInspector = selected
    ? {
        name: modifyName,
        id: String(selected.entity),
        components:
          seed.hierarchy.find((n) => n.entity === selected.entity)?.components ?? [],
        transform: seed.transformIdentity,
      }
    : inspector;

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
          This scene started from the <strong>{starter.title}</strong> template —
          its entities are yours to edit or delete. Press <strong>Play</strong> to
          run the template&rsquo;s starter game (Stop restores your scene). The
          Code panel shows the same starter code, but edits there don&rsquo;t
          change what Play runs yet — run edited code with <code>npm start</code>{" "}
          after saving the project to disk.
          {starter.github_link ? (
            <>
              {" · "}
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

  const diskSaveStatus =
    diskSave.phase === "saving" ? (
      <span className="editor-wizard__spinner" role="status">
        Saving your scene to disk…
      </span>
    ) : diskSave.phase === "saved" ? (
      <span className="editor-wizard__saved" role="status">
        {diskSave.via === "download"
          ? "Scene composite downloaded — re-save it over your project's main.composite."
          : `Wrote ${diskSave.filename} to disk (${diskSave.entities} entities).`}
      </span>
    ) : diskSave.phase === "canceled" ? (
      <span role="status" style={{ color: "var(--ink-7, rgba(255, 255, 255, 0.7))" }}>
        Save cancelled — nothing was written to disk.
      </span>
    ) : diskSave.phase === "error" ? (
      <span role="alert" style={{ color: "var(--error, #ff8080)" }}>
        Save failed: {diskSave.message}
      </span>
    ) : null;

  return (
    <div className="editor-wizard" data-step={step}>
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
        left={value === "browsingAssets" ? "assets" : "scene"}
        title={seed.scene.title}
        tree={tree}
        inspector={value === "modifying" ? modifyInspector : inspector}
        catalog={value === "browsingAssets" ? catalogItems : undefined}
        local={localAssets}
        viewportSrc={viewportSrc}
        addOpen={value === "addingComponent"}
        rawComposite={rawComposite}
        code={workspaceCode}
        prepareRealm={gameTemplateId ? prepareRealm : undefined}
        onEngineStatus={setEngineStatus}
      />

      {value === "placing" && (
        <DeNewEntityModal
          parent="root"
          parentName={inspector?.name ?? selectedNode?.name ?? "selection"}
          defaultName={placingName}
          onCancel={() => send({ type: "BACK" })}
          onCreate={() => send({ type: "CONFIRM_ENTITY", entity: 540 })}
        />
      )}

      {value === "editing" && (
        <>
          {localErrorBanner}
          {templateNotice}
          <Controls label={`Editing ${seed.scene.title}${seed.scene.live ? " — your deployed copy; changes stay in this browser until saved" : ""}`}>
            <button
              type="button"
              className="editor-wizard__btn editor-wizard__btn--primary"
              onClick={() => send({ type: "BROWSE_ASSETS" })}
            >
              Open Assets
            </button>
            <button
              type="button"
              className="editor-wizard__btn"
              onClick={onSaveToDisk}
              disabled={diskSave.phase === "saving" || enginePlaying}
              aria-busy={diskSave.phase === "saving"}
              title={
                enginePlaying
                  ? "Stop the scene first — changes made while it runs are temporary and aren't saved."
                  : undefined
              }
            >
              {diskSave.phase === "saving" ? "Saving…" : "Save to disk"}
            </button>
            {enginePlaying && (
              <span
                role="status"
                style={{ fontSize: "12px", color: "var(--ink-7, rgba(255, 255, 255, 0.7))" }}
              >
                Stop the scene to save — changes made while it runs are temporary.
              </span>
            )}
            {diskSaveStatus}
            {modifiable.length > 0 && (
              <EntityModifyButtons
                entities={modifiable}
                onModify={(n) => send({ type: "MODIFY_ENTITY", entity: n.entity, name: n.name })}
              />
            )}
          </Controls>
        </>
      )}

      {value === "modifying" && (
        <>
          <Controls label={`Modify "${modifyName}"${selected ? ` (#${selected.entity})` : ""}`}>
            <input
              className="editor-wizard__search"
              aria-label="Rename entity"
              placeholder="Rename entity"
              defaultValue={modifyName}
              onChange={(e) => {
                const name = e.currentTarget.value.trim();
                if (name) send({ type: "RENAME_ENTITY", name });
              }}
            />
            <div className="editor-wizard__axes" role="group" aria-label="Move axes">
              {(["x", "y", "z"] as const).map((axis) => (
                <button
                  key={axis}
                  type="button"
                  className={
                    "editor-wizard__btn" +
                    (state.context.transformAxes.includes(axis) ? " is-set" : "")
                  }
                  onClick={() => send({ type: "SET_TRANSFORM", axis })}
                >
                  Move {axis.toUpperCase()}
                </button>
              ))}
            </div>
            <ComponentButtons
              components={seed.components}
              onPick={(c) => send({ type: "ADD_COMPONENT", component: c.key })}
            />
            <button
              type="button"
              className="editor-wizard__btn editor-wizard__btn--primary"
              onClick={() => send({ type: "SAVE_EDIT" })}
            >
              Save changes
            </button>
            <button
              type="button"
              className="editor-wizard__btn editor-wizard__btn--danger"
              onClick={() => {
                if (
                  typeof window !== "undefined" &&
                  !window.confirm(`Delete "${modifyName}"? This can't be undone.`)
                ) {
                  return;
                }
                send({ type: "DELETE_ENTITY" });
              }}
            >
              Delete entity
            </button>
            <button
              type="button"
              className="editor-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back
            </button>
          </Controls>
        </>
      )}

      {value === "browsingAssets" && (
        <>
          <Controls label="Search the catalog, then place a model">
            <input
              className="editor-wizard__search"
              placeholder="Search models"
              aria-label="Search models"
              defaultValue={state.context.query ?? ""}
              onChange={(e) => send({ type: "SEARCH", query: e.currentTarget.value.trim() })}
            />
            <AssetButtons
              models={guidedModels}
              onSelect={(a) => send({ type: "SELECT_ASSET", id: a.id, name: a.name })}
              onPlace={(a) => send({ type: "PLACE_ASSET", id: a.id, name: a.name })}
            />
            {assetQuery && guidedModels.length === 0 ? (
              <span role="status" style={{ color: "var(--ink-7, rgba(255, 255, 255, 0.7))" }}>
                No models match &ldquo;{state.context.query}&rdquo;
              </span>
            ) : null}
            <button
              type="button"
              className="editor-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back
            </button>
          </Controls>
        </>
      )}

      {value === "placing" && (
        <>
          <Controls label={`Create a new entity from "${placingName}"`}>
            <button
              type="button"
              className="editor-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Cancel
            </button>
            <button
              type="button"
              className="editor-wizard__btn editor-wizard__btn--primary"
              onClick={() => send({ type: "CONFIRM_ENTITY", entity: 540 })}
            >
              Create entity
            </button>
          </Controls>
        </>
      )}

      {value === "transforming" && (
        <>
          <Controls label={`Set Transform on "${placedName}"`}>
            <div className="editor-wizard__axes" role="group" aria-label="Transform axes">
              {(["x", "y", "z"] as const).map((axis) => (
                <button
                  key={axis}
                  type="button"
                  className={
                    "editor-wizard__btn" +
                    (state.context.transformAxes.includes(axis) ? " is-set" : "")
                  }
                  onClick={() => send({ type: "SET_TRANSFORM", axis })}
                >
                  Position {axis.toUpperCase()}
                </button>
              ))}
            </div>
            <button
              type="button"
              className="editor-wizard__btn editor-wizard__btn--primary"
              onClick={() => send({ type: "SAVE_WITHOUT_COMPONENT" })}
            >
              Save
            </button>
            <button
              type="button"
              className="editor-wizard__btn"
              onClick={() => send({ type: "ADD_COMPONENT_PANEL" })}
            >
              Add a component
            </button>
            <button
              type="button"
              className="editor-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back to assets
            </button>
          </Controls>
        </>
      )}

      {value === "addingComponent" && (
        <>
          <Controls label={`Add a component to "${placedName}"`}>
            <ComponentButtons
              components={seed.components}
              onPick={(c) => send({ type: "PICK_COMPONENT", component: c.key })}
            />
            <button
              type="button"
              className="editor-wizard__btn"
              onClick={() => send({ type: "BACK" })}
            >
              Back
            </button>
          </Controls>
        </>
      )}

      {value === "saving" && (
        <>
          <Controls label={previewSave ? "Applying…" : "Saving…"}>
            <span className="editor-wizard__spinner" aria-live="polite">
              {previewSave ? "Applying your changes…" : "Saving your changes…"}
            </span>
          </Controls>
        </>
      )}

      {value === "saved" && (
        <>
          <Controls label={previewSave ? "Preview updated" : "Saved"}>
            <span className="editor-wizard__saved" role="status">
              {previewSave ? "Changes applied in this preview." : savedDetail}
            </span>
            <button
              type="button"
              className="editor-wizard__btn editor-wizard__btn--primary"
              onClick={() => {
                setSearchParams(
                  (prev) => {
                    const params = new URLSearchParams(prev);
                    params.set("step", stateToSlug("editing"));
                    return params;
                  },
                  { preventScrollReset: true },
                );
                onRestart();
              }}
            >
              Continue editing
            </button>
          </Controls>
          {previewSave && (
            <p
              className="editor-wizard__note"
              role="note"
              style={{
                margin: "8px 0 0",
                fontSize: "12px",
                lineHeight: 1.5,
                color: "var(--ink-7, rgba(255, 255, 255, 0.7))",
              }}
            >
              Preview — changes aren&rsquo;t saved yet. This build has no
              composite-write backend, so your edits live only in this preview
              session and aren&rsquo;t persisted to the scene. Use{" "}
              <strong>Save to disk</strong> to export the scene composite.
            </p>
          )}
        </>
      )}

      {value === "error" && (
        <>
          <Controls label={`Save failed: ${state.context.error ?? "unknown error"}`}>
            <button
              type="button"
              className="editor-wizard__btn editor-wizard__btn--primary"
              onClick={() => send({ type: "RETRY" })}
            >
              Retry save
            </button>
          </Controls>
        </>
      )}
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

function AssetButtons({
  models,
  onSelect,
  onPlace,
}: {
  models: Asset[];
  onSelect: (a: Asset) => void;
  onPlace: (a: Asset) => void;
}) {
  return (
    <div className="editor-wizard__assets">
      {models.slice(0, 4).map((a) => (
        <button
          key={a.id}
          type="button"
          className="editor-wizard__btn"
          onMouseEnter={() => onSelect(a)}
          onClick={() => onPlace(a)}
          title={a.src}
        >
          Place {a.name}
        </button>
      ))}
    </div>
  );
}

function EntityModifyButtons({
  entities,
  onModify,
}: {
  entities: HierarchyNode[];
  onModify: (n: HierarchyNode) => void;
}) {
  return (
    <div className="editor-wizard__entities" role="group" aria-label="Modify an entity">
      {entities.slice(0, 5).map((n) => (
        <button
          key={n.entity}
          type="button"
          className="editor-wizard__btn"
          onClick={() => onModify(n)}
          title={`Modify ${n.name} (#${n.entity})`}
        >
          Modify {n.name}
        </button>
      ))}
    </div>
  );
}

function ComponentButtons({
  components,
  onPick,
}: {
  components: ComponentDef[];
  onPick: (c: ComponentDef) => void;
}) {
  return (
    <div className="editor-wizard__components">
      {components.slice(0, 6).map((c) => (
        <button
          key={c.key}
          type="button"
          className="editor-wizard__btn"
          onClick={() => onPick(c)}
          title={c.componentName}
        >
          {c.label}
        </button>
      ))}
    </div>
  );
}
