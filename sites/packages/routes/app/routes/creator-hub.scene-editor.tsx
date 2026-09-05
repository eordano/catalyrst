import { useEffect, useState } from "react";
import { useNavigate } from "react-router";

import {
  loadSceneEditorSeed,
  newSceneSeed,
  emptySeed,
  seedFromCompositeJSON,
  buildViewportUrl,
  type SceneEditorSeed,
} from "@data/lib/catalyst/creator-hub/scene-editor";
import {
  loadAssetCatalog,
  type CatalogItem,
} from "@data/lib/catalyst/creator-hub/asset-catalog.server";
import {
  buildTemplateCompositeText,
  hasTemplateComposite,
} from "@data/lib/fs/template-composites";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import { resolveBreadcrumbOrigin } from "@features/components/creator-hub/breadcrumbOrigins";
import EditorWizard from "@features/stories/creator-hub/scene-editor-place-items/EditorWizard";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.scene-editor";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Scene editor");

const STORY: StoryId = "creator-hub/scene-editor-place-items";

export function headers(): Record<string, string> {
  return {
    "Cross-Origin-Opener-Policy": "same-origin",
    "Cross-Origin-Embedder-Policy": "credentialless",
  };
}

const FALLBACK: Assignment = {
  variant: "guided",
  flags: { guided: true },
  experimentKey: "ch_editor_place_items",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const pointer = url.searchParams.get("pointer")?.trim() || undefined;
  const isLocal = url.searchParams.get("source") === "local";
  const isNew = url.searchParams.get("new") === "1" || (!isLocal && !pointer);
  const newName = url.searchParams.get("name")?.trim() || undefined;
  const newTemplate = url.searchParams.get("template")?.trim() || undefined;
  const newLayout = url.searchParams.get("layout")?.trim() || undefined;
  const projectSlug = url.searchParams.get("project")?.trim() || undefined;
  const from = url.searchParams.get("from")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let seed: SceneEditorSeed;
  let rawComposite: string | undefined;
  if (isNew) {
    seed = newSceneSeed({ name: newName, template: newTemplate, layout: newLayout });
    if (newTemplate && hasTemplateComposite(newTemplate)) {
      const text = buildTemplateCompositeText(newTemplate);
      if (text) {
        try {
          seed = seedFromCompositeJSON(JSON.parse(text), seed, []);
          rawComposite = text;
        } catch {
        }
      }
    }
  } else if (isLocal) {
    seed = emptySeed(pointer);
  } else {
    try {
      seed = await loadSceneEditorSeed({ pointer, signal: request.signal });
    } catch {
      seed = emptySeed(pointer);
    }
  }

  const editorSceneUrl =
    process.env.BEVY_EDITOR_SCENE_URL ||
    process.env.EDITOR_SYSTEM_SCENE_URL ||
    "/_editor-scene";

  const useProjectRealm = isNew || isLocal;
  const projectRealm = process.env.BEVY_PROJECT_REALM_URL || "/_project";
  const publicRealm =
    process.env.REALM_URL ||
    process.env.PUBLIC_CATALYST_URL ||
    "https://catalyst.example.com";
  const projectBase = seed.scene.base || "0,0";
  // The editor viewport IS the preview launch plus the system scene that owns
  // the editing UI. Built from one object so the two cannot drift onto
  // different realms or parcels -- editing one parcel while previewing another
  // is the kind of bug that reads as "the editor saved nothing".
  const launch = {
    playUrl: process.env.BEVY_PLAY_URL || "/_play",
    realm: useProjectRealm ? projectRealm : publicRealm,
    position: useProjectRealm ? projectBase : seed.scene.base || pointer || "0,0",
    preview: useProjectRealm,
  };
  const viewportSrc = buildViewportUrl({ ...launch, systemScene: editorSceneUrl, editorUi: true });
  const previewSrc = buildViewportUrl(launch);

  // null when the asset packs did not answer; the wizard falls back to the
  // bundled seed catalog rather than drawing an empty asset browser.
  const catalog: CatalogItem[] | undefined =
    (await loadAssetCatalog({ signal: request.signal })) ?? undefined;

  const payload = {
    sid,
    step,
    assignment,
    seed,
    viewportSrc,
    previewSrc,
    from,
    projectSlug,
    catalog,
    ...(rawComposite ? { rawComposite } : {}),
  };
  return wrap(payload);
}

type LoaderData = {
  sid: string;
  step: string | null;
  assignment: Assignment;
  seed: SceneEditorSeed;
  viewportSrc: string;
  previewSrc?: string;
  from?: string | null;
  projectSlug?: string;
  rawComposite?: string;
  draftAssets?: Record<string, string>;
  catalog?: CatalogItem[];
  failedToLoadLocal?: boolean;
};

const COMPOSITE_RE = /(^|\/)main\.composite$/i;
const SCENE_JSON_RE = /(^|\/)scene\.json$/i;
const GLB_RE = /\.(glb|gltf)$/i;

function safeJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function baseSeedFromSceneJson(sceneJsonText: string | null): SceneEditorSeed {
  const j = safeJson(sceneJsonText ?? "") as
    | {
        display?: { title?: string };
        scene?: { base?: string; parcels?: string[] };
        tags?: string[];
      }
    | null;
  const base = j?.scene?.base ?? j?.scene?.parcels?.[0] ?? "0,0";
  const seed = emptySeed(base);
  const tag = j?.tags?.[0];
  const template = tag && hasTemplateComposite(tag) ? tag : undefined;
  seed.scene = {
    ...seed.scene,
    title: j?.display?.title?.trim() || "Local scene",
    base,
    parcels: j?.scene?.parcels?.length ? j.scene.parcels : seed.scene.parcels,
    ...(template ? { template } : {}),
  };
  return seed;
}

async function seedFromProjectMeta(
  slug: string,
  fallback: SceneEditorSeed,
): Promise<{
  seed: SceneEditorSeed;
  rawComposite?: string;
  draftAssets?: Record<string, string>;
  failedToLoadLocal?: boolean;
} | null> {
  try {
    const { handleStore } = await import("@data/lib/fs/handle-store");
    const meta = await handleStore.getMeta(slug);
    if (!meta) return null;
    const baseSeed: SceneEditorSeed = {
      ...fallback,
      scene: {
        ...fallback.scene,
        title: meta.title || fallback.scene.title,
        base: meta.base || fallback.scene.base,
        ...(meta.template && hasTemplateComposite(meta.template)
          ? { template: meta.template }
          : {}),
      },
    };
    const draftAssets =
      meta.assets && Object.keys(meta.assets).length > 0 ? meta.assets : undefined;
    if (!meta.composite) return { seed: baseSeed, ...(draftAssets ? { draftAssets } : {}) };
    const parsed = safeJson(meta.composite);
    if (parsed === null) {
      return { seed: baseSeed, failedToLoadLocal: true, ...(draftAssets ? { draftAssets } : {}) };
    }
    const seed = seedFromCompositeJSON(parsed, baseSeed, []);
    return { seed, rawComposite: meta.composite, ...(draftAssets ? { draftAssets } : {}) };
  } catch {
    return null;
  }
}

async function seedFromPersistedHandle(
  slug: string,
): Promise<{ seed: SceneEditorSeed; rawComposite?: string; failedToLoadLocal?: boolean } | null> {
  try {
    const { handleStore, ensureHandlePermission } = await import("@data/lib/fs/handle-store");
    const handle = await handleStore.get(slug);
    if (!handle) return null;
    if (!(await ensureHandlePermission(handle, "read"))) return null;
    const { readDirectoryFiles, readText } = await import("@data/lib/fs/disk");
    const files = await readDirectoryFiles(handle);
    const paths = Object.keys(files);
    const compPath = paths.find((p) => COMPOSITE_RE.test(p));
    if (!compPath) return null;
    const compositeText = await readText(files[compPath]);
    const sjPath = paths.find((p) => SCENE_JSON_RE.test(p));
    const sceneJsonText = sjPath ? await readText(files[sjPath]) : null;
    const glbs = paths.filter((p) => GLB_RE.test(p));
    const parsed = safeJson(compositeText);
    if (parsed === null) {
      return { seed: baseSeedFromSceneJson(sceneJsonText), failedToLoadLocal: true };
    }
    const seed = seedFromCompositeJSON(parsed, baseSeedFromSceneJson(sceneJsonText), glbs);
    return { seed, rawComposite: compositeText };
  } catch {
    return null;
  }
}

export async function clientLoader({ request, serverLoader }: Route.ClientLoaderArgs) {
  const base = (await serverLoader()) as LoaderData;
  const url = new URL(request.url);

  const draftSlug = url.searchParams.get("draft")?.trim();
  if (draftSlug) {
    const seededDraft = await seedFromProjectMeta(draftSlug, base.seed);
    if (seededDraft) return { ...base, ...seededDraft };
  }

  if (url.searchParams.get("source") !== "local") return base;

  const projectSlug = url.searchParams.get("project")?.trim();
  if (projectSlug) {
    const seeded = await seedFromPersistedHandle(projectSlug);
    if (seeded) return { ...base, ...seeded };
    const seededMeta = await seedFromProjectMeta(projectSlug, base.seed);
    if (seededMeta) return { ...base, ...seededMeta };
  }

  const { readLocalSeed, readLocalComposite } = await import("@data/lib/fs/local-scene");
  const seed = readLocalSeed();
  const rawComposite = readLocalComposite() ?? undefined;
  if (seed) return { ...base, seed, rawComposite };
  return base;
}

clientLoader.hydrate = true as const;

export function HydrateFallback() {
  return (
    <main className="creator-hub-scene-editor">
      <div
        role="status"
        style={{
          minHeight: "100dvh",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background: "var(--lm-bg, #0d0c11)",
          color: "var(--lm-ink-3, rgba(255,255,255,0.7))",
          fontSize: "var(--fs-15)",
        }}
      >
        Loading scene editor&#x2026;
      </div>
    </main>
  );
}

const EDITOR_MIN_WIDTH = 900;

function useBelowEditorWidth(): boolean {
  const [narrow, setNarrow] = useState(false);
  useEffect(() => {
    const mq = window.matchMedia(`(max-width: ${EDITOR_MIN_WIDTH - 1}px)`);
    const sync = () => setNarrow(mq.matches);
    sync();
    mq.addEventListener("change", sync);
    return () => mq.removeEventListener("change", sync);
  }, []);
  return narrow;
}

function SceneEditorDesktopGate({ onExit }: { onExit: () => void }) {
  const [copied, setCopied] = useState(false);
  const copyLink = async () => {
    const href = window.location.href;
    try {
      await navigator.clipboard.writeText(href);
      setCopied(true);
      return;
    } catch {
    }
    try {
      const ta = document.createElement("textarea");
      ta.value = href;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
      setCopied(true);
    } catch {
    }
  };
  return (
    <div
      role="status"
      style={{
        minHeight: "100dvh",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: "var(--s-4)",
        padding: "var(--s-6) var(--s-5)",
        textAlign: "center",
        background: "var(--lm-bg, #0d0c11)",
        color: "#fff",
      }}
    >
      <h1 style={{ margin: 0, fontSize: "var(--fs-24)", fontWeight: "var(--fw-bold)" }}>
        Continue on desktop
      </h1>
      <p
        style={{
          margin: 0,
          maxWidth: 420,
          fontSize: "var(--fs-15)",
          lineHeight: 1.5,
          color: "var(--lm-ink-3, rgba(255,255,255,0.7))",
        }}
      >
        The scene editor needs a larger screen to place items, arrange the 3D
        viewport and manage your scene. Open this page on a desktop or laptop to
        keep building.
      </p>
      <button
        type="button"
        onClick={copyLink}
        style={{
          marginTop: "var(--s-2)",
          padding: "var(--s-2-5) var(--s-5)",
          borderRadius: 999,
          border: "none",
          background: "var(--brand-cta)",
          color: "#fff",
          fontSize: "var(--fs-15)",
          fontWeight: "var(--fw-semibold)",
          cursor: "pointer",
        }}
      >
        {copied ? "Link copied" : "Copy link for desktop"}
      </button>
      {copied && (
        <p
          role="status"
          style={{
            margin: 0,
            fontSize: "var(--fs-13)",
            color: "var(--lm-ink-3, rgba(255,255,255,0.7))",
          }}
        >
          Open the copied link in a desktop browser to continue this scene.
        </p>
      )}
      <button
        type="button"
        onClick={onExit}
        style={{
          padding: "var(--s-2-5) var(--s-5)",
          borderRadius: 999,
          border: "1px solid rgba(255,255,255,0.3)",
          background: "transparent",
          color: "#fff",
          fontSize: "var(--fs-15)",
          fontWeight: "var(--fw-semibold)",
          cursor: "pointer",
        }}
      >
        Back to Creator Hub
      </button>
    </div>
  );
}

export default function CreatorHubSceneEditor({ loaderData }: Route.ComponentProps) {
  const { seed, viewportSrc, previewSrc, rawComposite, draftAssets, from, projectSlug, catalog, failedToLoadLocal } =
    loaderData;
  const navigate = useNavigate();

  const handlePublish = (_id?: string, draft?: string) => {
    const isCoords = /^-?\d+\s*,\s*-?\d+$/.test(seed.scene.pointer ?? "");
    const isWorld = seed.scene.live && Boolean(seed.scene.pointer) && !isCoords;
    const proj = projectSlug ? `&project=${encodeURIComponent(projectSlug)}` : "";
    const world =
      isWorld && seed.scene.pointer
        ? `&world=${encodeURIComponent(seed.scene.pointer)}`
        : "";
    const draftQ = draft ? `&draft=${encodeURIComponent(draft)}` : "";
    navigate("/creator-hub/deploy-world?from=scene-editor" + world + proj + draftQ);
  };

  const handleExit = () => navigate(resolveBreadcrumbOrigin(from ?? null).to);

  const belowEditorWidth = useBelowEditorWidth();
  if (belowEditorWidth) {
    return (
      <main className="creator-hub-scene-editor">
        <SceneEditorDesktopGate onExit={handleExit} />
      </main>
    );
  }

  return (
    <main className="creator-hub-scene-editor">
      <EditorWizard
        seed={seed}
        viewportSrc={viewportSrc}
        previewSrc={previewSrc}
        rawComposite={rawComposite}
        draftAssets={draftAssets}
        catalog={catalog}
        failedToLoadLocal={failedToLoadLocal}
        onExit={handleExit}
        onPublish={handlePublish}
      />
    </main>
  );
}
