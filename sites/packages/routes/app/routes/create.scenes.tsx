import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useRevalidator, useSearchParams } from "react-router";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import ChScenes, { type ChScenesTab } from "@ui/creatorhub/pages/ChScenes";
import ChMyScenes, { type MySceneVM } from "@ui/creatorhub/pages/ChMyScenes";
import ChScenesEmptyState from "@ui/creatorhub/pages/ChScenesEmptyState";
import ChModalDeleteProject from "@ui/creatorhub/components/ChModalDeleteProject";
import "@ui/creatorhub/components/chmodaldeleteproject.css";

import { useAuth } from "@data/lib/auth/index";
import {
  handleStore,
  slugifyProjectTitle,
  ensureHandlePermission,
  type ProjectMeta,
} from "@data/lib/fs/handle-store";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { loadCreatorScenes, type CreatorScene } from "@data/lib/catalyst/create/index.server";
import { discoverDeployedScenes } from "@data/lib/catalyst/creator-hub/discover-deployed.server";
import type { DiscoveredScene } from "@data/lib/catalyst/creator-hub/discover-deployed";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { track } from "@core/lib/telemetry/track";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/create.scenes";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Scenes");

const STORY: StoryId = "create/hub-to-scenes";

const WALLET_RE = /^0x[0-9a-f]{40}$/;

function sceneHue(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360;
  return h;
}

type ProjectCardVM = {
  id: string;
  title: string;
  thumbnail: string | undefined;
  layout: { cols: number; rows: number };
  grad: string;
  published: boolean;
  hasDeployments: boolean;
  local?: boolean;
  slug?: string;
};

function grad(seed: string): string {
  const h = sceneHue(seed);
  return `linear-gradient(135deg, hsl(${h} 70% 52%), hsl(${(h + 40) % 360} 60% 28%))`;
}

type PendingLocalDelete = { slug: string; title: string; hasFolder: boolean };

async function deleteLocalProjectFiles(slug: string): Promise<boolean> {
  try {
    const handle = await handleStore.get(slug);
    if (!handle) return false;
    if (!(await ensureHandlePermission(handle, "readwrite"))) return false;
    const names: string[] = [];
    for await (const name of (
      handle as unknown as { keys(): AsyncIterable<string> }
    ).keys()) {
      names.push(name);
    }
    for (const name of names) {
      await handle.removeEntry(name, { recursive: true });
    }
    return true;
  } catch {
    return false;
  }
}

function toProjectCard(s: CreatorScene): ProjectCardVM {
  return {
    id: s.id,
    title: s.title,
    thumbnail: s.image ?? undefined,
    layout: { cols: Math.max(1, s.parcels), rows: 1 },
    grad: grad(s.id),
    published: true,
    hasDeployments: false,
  };
}

function toLocalCard(m: ProjectMeta): ProjectCardVM {
  return {
    id: `local:${m.slug}`,
    slug: m.slug,
    local: true,
    title: m.title || m.slug,
    thumbnail: m.thumbnail,
    layout: { cols: Math.max(1, m.parcels ?? 1), rows: 1 },
    grad: grad(m.slug),
    published: false,
    hasDeployments: false,
  };
}

const FALLBACK: Assignment = {
  variant: "scenes_first",
  flags: { scenesFirst: true },
  experimentKey: "ch_hub_to_scenes",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const view = url.searchParams.get("view") === "published" ? "published" : "local";
  const creator = (url.searchParams.get("creator")?.trim() || readWallet(request)) ?? "";
  const forceEmpty =
    typeof process !== "undefined" &&
    process.env?.NODE_ENV !== "production" &&
    url.searchParams.get("state") === "empty";

  const asParam = (url.searchParams.get("as") ?? "").trim().toLowerCase();
  const devOverride = import.meta.env.DEV && WALLET_RE.test(asParam) ? asParam : "";
  const publishedAddress = devOverride || readWallet(request) || "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let scenes: CreatorScene[] = [];
  let error = false;
  if (!forceEmpty && view === "local") {
    try {
      scenes = await loadCreatorScenes({ creator: creator || undefined, limit: 12 });
    } catch {
      error = true;
    }
  }

  const published: DiscoveredScene[] =
    view === "published" && publishedAddress
      ? await discoverDeployedScenes(publishedAddress, { signal: request.signal }).catch(
          () => [],
        )
      : [];

  const payload = {
    sid,
    creator,
    scenes,
    error,
    view,
    publishedAddress,
    dev: Boolean(devOverride),
    published,
  };
  return wrap(payload);
}

function useSceneTabs(active: "local" | "published") {
  const [, setSearchParams] = useSearchParams();
  const tabs: ChScenesTab[] = [
    { id: "local", label: "Drafts & local", active: active === "local" },
    { id: "published", label: "Published", active: active === "published" },
  ];
  const onTab = (id: string) => {
    if (id === active) return;
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (id === "published") next.set("view", "published");
        else next.delete("view");
        return next;
      },
      { preventScrollReset: true },
    );
  };
  return { tabs, onTab };
}

export default function CreateScenes({ loaderData }: Route.ComponentProps) {
  const d = loaderData;

  if (d.view === "published") {
    return (
      <PublishedScenesView
        sid={d.sid}
        loaderAddress={d.publishedAddress}
        dev={d.dev}
        scenes={d.published}
      />
    );
  }
  return <ScenesView sid={d.sid} creator={d.creator} scenes={d.scenes} error={d.error} />;
}

function normalizeDeployedTs(ts: number | null): number {
  if (!ts) return 0;
  return ts < 1e12 ? ts * 1000 : ts;
}

function PublishedScenesView({
  sid,
  loaderAddress,
  dev,
  scenes,
}: {
  sid: string;
  loaderAddress: string;
  dev: boolean;
  scenes: DiscoveredScene[];
}) {
  const navigate = useNavigate();
  const revalidator = useRevalidator();
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);
  const { tabs, onTab } = useSceneTabs("published");

  const needsRescope = isConnected && !!address && !loaderAddress;
  useEffect(() => {
    if (needsRescope && revalidator.state === "idle") {
      if (typeof revalidator.revalidate === "function") revalidator.revalidate();
    }
  }, [needsRescope, revalidator]);

  const [localMetas, setLocalMetas] = useState<ProjectMeta[]>([]);
  useEffect(() => {
    let cancelled = false;
    handleStore
      .list()
      .then((metas) => {
        if (!cancelled && metas) setLocalMetas(metas);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const [downloadingId, setDownloadingId] = useState<string | null>(null);

  const vms: MySceneVM[] = useMemo(
    () =>
      scenes.map((s) => {
        const meta = localMetas.find((m) => m.slug === slugifyProjectTitle(s.title));
        if (!meta) return s;
        const syncState =
          (meta.updatedAt ?? 0) > normalizeDeployedTs(s.deployedAt)
            ? ("local-ahead" as const)
            : ("local" as const);
        return { ...s, syncState };
      }),
    [scenes, localMetas],
  );

  const signedIn = dev || isConnected || !!loaderAddress;
  const account = loaderAddress || address || "";
  const loading = needsRescope || revalidator.state === "loading";

  async function onDownload(s: MySceneVM) {
    if (downloadingId) return;
    setDownloadingId(s.entityId);
    try {
      const { downloadDeployedEntityZip } = await import("@data/lib/fs/save-scene");
      const ok = await downloadDeployedEntityZip(s.entityId, s.title);
      track(
        "ch_scenes_clicked",
        { to: "download-entity-zip", ok, entity: s.entityId },
        { sid, story: STORY },
      );
    } finally {
      setDownloadingId(null);
    }
  }

  function onRepublish(s: MySceneVM) {
    const meta = localMetas.find((m) => m.slug === slugifyProjectTitle(s.title));
    const q = new URLSearchParams();
    q.set("from", "scenes");
    if (meta) q.set("project", meta.slug);
    track(
      "ch_scenes_clicked",
      { to: "republish-land", entity: s.entityId, parcel: s.baseParcel },
      { sid, story: STORY },
    );
    navigate(`/creator-hub/deploy-world?${q.toString()}`);
  }

  return (
    <ChMyScenes
      signedIn={signedIn}
      account={account}
      name={name}
      loading={loading}
      scenes={vms}
      onSignIn={() => openSignIn()}
      onOpen={(s) => navigate(s.openHref)}
      onStartFromTemplate={() => navigate("/create/templates")}
      onDownload={(s) => void onDownload(s)}
      onRepublish={onRepublish}
      downloadingId={downloadingId}
      tabs={tabs}
      onTab={onTab}
    />
  );
}

function ScenesView({
  sid,
  creator,
  scenes,
  error,
}: {
  sid: string;
  creator: string;
  scenes: CreatorScene[];
  error: boolean;
}) {
  const navigate = useNavigate();
  const revalidator = useRevalidator();
  const [, setSearchParams] = useSearchParams();
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);
  const { tabs, onTab } = useSceneTabs("local");
  const isEmpty = scenes.length === 0;
  const rescoping = isConnected && Boolean(address) && !creator;

  const [localCards, setLocalCards] = useState<ProjectCardVM[]>([]);
  const [pendingDelete, setPendingDelete] = useState<PendingLocalDelete | null>(null);
  const [deleteNotice, setDeleteNotice] = useState<string | null>(null);

  const [importPhase, setImportPhase] = useState<
    "picking" | "reading" | "opening" | null
  >(null);
  const [importError, setImportError] = useState<string | null>(null);

  useEffect(() => {
    if (!deleteNotice) return undefined;
    const t = setTimeout(() => setDeleteNotice(null), 8000);
    return () => clearTimeout(t);
  }, [deleteNotice]);
  useEffect(() => {
    let cancelled = false;
    const load = () => {
      handleStore
        .list()
        .then((metas) => {
          if (!cancelled && metas) setLocalCards(metas.map(toLocalCard));
        })
        .catch(() => {});
    };
    load();
    if (typeof window === "undefined") return undefined;
    const onFocus = () => load();
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onFocus);
    return () => {
      cancelled = true;
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onFocus);
    };
  }, []);

  const publishedCards = scenes.map(toProjectCard);
  const publishedSlugs = new Set(scenes.map((s) => slugifyProjectTitle(s.title)));
  const mergedCards = [
    ...localCards.filter((c) => c.slug && !publishedSlugs.has(c.slug)),
    ...publishedCards,
  ];
  const hasCards = mergedCards.length > 0;

  useEffect(() => {
    if (isConnected && address && !creator) {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("creator", address);
          return next;
        },
        { replace: true, preventScrollReset: true },
      );
    }
  }, [isConnected, address, creator, setSearchParams]);

  const fired = useRef(false);
  useEffect(() => {
    if (fired.current) return;
    fired.current = true;
    track(
      "ch_scenes_viewed",
      { count: scenes.length, empty: isEmpty, error },
      { sid, story: STORY },
    );
    if (isEmpty && !error) {
      track("ch_scenes_empty_viewed", {}, { sid, story: STORY });
    }
  }, [sid, scenes.length, isEmpty, error]);

  function onSelectScene(id: string) {
    if (id.startsWith("local:")) {
      const slug = id.slice("local:".length);
      track("ch_scenes_clicked", { to: "editor", local: true }, { sid, story: STORY });
      const to = `/creator-hub/scene-editor?source=local&project=${encodeURIComponent(slug)}&from=scenes`;
      if (typeof window !== "undefined") window.location.assign(to);
      else navigate(to);
      return;
    }
    const scene = scenes.find((s) => s.id === id);
    const pointer = scene?.base_position;
    track("ch_scenes_clicked", { to: "editor", seeded: Boolean(pointer) }, { sid, story: STORY });
    navigate(
      pointer
        ? `/creator-hub/scene-editor?pointer=${encodeURIComponent(pointer)}&from=scenes`
        : "/creator-hub/scene-editor?from=scenes",
    );
  }
  function onCreateScene() {
    track("ch_scenes_clicked", { to: "scene-editor" }, { sid, story: STORY });
    if (typeof window !== "undefined") {
      window.location.assign("/creator-hub/scene-editor?new=1&from=scenes");
    } else {
      navigate("/creator-hub/scene-editor?new=1&from=scenes");
    }
  }
  function onDeleteScene(id: string) {
    if (id.startsWith("local:")) {
      const slug = id.slice("local:".length);
      const card = localCards.find((c) => c.slug === slug);
      track("ch_scenes_clicked", { to: "delete-local", local: true }, { sid, story: STORY });
      handleStore
        .get(slug)
        .catch(() => null)
        .then((handle) => {
          setPendingDelete({
            slug,
            title: card?.title ?? slug,
            hasFolder: Boolean(handle),
          });
          track(
            "ch_delete_opened",
            { project_id: id, published: false, local: true },
            { sid, story: STORY },
          );
        });
      return;
    }
    track("ch_scenes_clicked", { to: "delete-project" }, { sid, story: STORY });
    const params = new URLSearchParams({ step: "confirm", project: id });
    if (address) params.set("creator", address);
    params.set("from", "scenes");
    navigate(`/creator-hub/delete-project?${params}`);
  }
  function onCancelLocalDelete() {
    if (!pendingDelete) return;
    track(
      "ch_delete_cancelled",
      { project_id: `local:${pendingDelete.slug}`, local: true },
      { sid, story: STORY },
    );
    setPendingDelete(null);
  }
  async function onConfirmLocalDelete(shouldDeleteFiles: boolean) {
    if (!pendingDelete) return;
    const { slug, title, hasFolder } = pendingDelete;

    let filesOutcome: "deleted" | "kept" = "kept";
    if (shouldDeleteFiles && hasFolder) {
      track(
        "ch_delete_files_opted_in",
        { project_id: `local:${slug}`, local: true },
        { sid, story: STORY },
      );
      filesOutcome = (await deleteLocalProjectFiles(slug)) ? "deleted" : "kept";
    }

    await handleStore.clear(slug).catch(() => {});
    setLocalCards((prev) => prev.filter((c) => c.slug !== slug));
    setPendingDelete(null);
    track(
      "ch_delete_confirmed",
      {
        project_id: `local:${slug}`,
        delete_files: shouldDeleteFiles,
        local_files: hasFolder ? filesOutcome : undefined,
        local: true,
        simulated: false,
      },
      { sid, story: STORY },
    );
    setDeleteNotice(
      hasFolder
        ? filesOutcome === "deleted"
          ? `"${title}" was removed from My Scenes and its files were deleted from the connected folder.`
          : `"${title}" was removed from My Scenes. Its files are still on your computer \u{2014} use Import to bring it back.`
        : `"${title}" was removed from My Scenes.`,
    );
  }
  function onSignIn() {
    track("ch_scenes_signin_clicked", {}, { sid, story: STORY });
    openSignIn();
  }
  function onTemplates() {
    track("ch_scenes_clicked", { to: "templates" }, { sid, story: STORY });
    navigate("/create/templates");
  }
  async function onImport() {
    if (importPhase) return;
    track("ch_scenes_clicked", { to: "import-local" }, { sid, story: STORY });
    setImportError(null);
    setImportPhase("picking");
    try {
      const { openLocalScene, stashLocalSeed, stashCompositeHandle, stashLocalComposite } =
        await import("@data/lib/fs/local-scene");
      const out = await openLocalScene({ onPhase: setImportPhase });
      if (out.status === "cancelled") {
        setImportPhase(null);
        return;
      }
      if (out.status === "no-composite") {
        setImportPhase(null);
        track(
          "ch_scenes_import_failed",
          { reason: "no_composite", has_scene_json: out.hasSceneJson, files: out.fileCount },
          { sid, story: STORY },
        );
        setImportError(
          out.hasSceneJson
            ? "No main.composite found in that folder \u{2014} it looks like a code-only SDK project (scene.json but no saved scene). Pick the project folder that contains your saved scene, the one holding main.composite."
            : "No main.composite found in that folder \u{2014} pick the project folder that contains your saved scene, the one holding main.composite.",
        );
        return;
      }
      const res = out.result;
      setImportPhase("opening");
      stashLocalSeed(res.seed);
      await stashCompositeHandle(res.compositeHandle);
      stashLocalComposite(res.compositeText);
      {
        const { populateProjectRealm, clearProjectRealm } = await import(
          "@data/lib/fs/project-realm"
        );
        if (res.files) {
          const pr = await populateProjectRealm(res.files, res.sceneJsonText);
          if (!pr.ok) await clearProjectRealm();
        } else {
          await clearProjectRealm();
        }
      }
      if (typeof window !== "undefined") {
        window.location.assign("/creator-hub/scene-editor?source=local&from=scenes");
      } else {
        navigate("/creator-hub/scene-editor?source=local&from=scenes");
      }
    } catch {
      setImportPhase(null);
      track("ch_scenes_import_failed", { reason: "exception" }, { sid, story: STORY });
      setImportError(
        "Couldn't read that folder \u{2014} check that this site still has permission to access it, then try again.",
      );
    }
  }
  function onRetry() {
    track("ch_scenes_clicked", { to: "retry" }, { sid, story: STORY });
    if (typeof revalidator.revalidate === "function") {
      revalidator.revalidate();
    } else {
      navigate(0);
    }
  }

  const retrying = revalidator.state === "loading";

  return (
    <div className="create-scenes">
      {error && !hasCards ? (
        <CreatorHubChrome active="scenes" signedIn={isConnected} account={address ?? ""} name={name} onSignIn={onSignIn}>
          <div className="create-scenes__errorwrap" style={SCENES_ERROR_WRAP}>
            <div className="create-scenes__error" role="alert" style={SCENES_ERROR}>
              <span>
                Couldn&#x2019;t load your scenes just now &#x2014; the Builder data layer may be
                temporarily unavailable. Your work isn&#x2019;t lost; try again in a moment.
              </span>
              <button
                type="button"
                onClick={onRetry}
                disabled={retrying}
                style={SCENES_RETRY}
              >
                {retrying ? "Retrying\u{2026}" : "Try again"}
              </button>
            </div>
          </div>
        </CreatorHubChrome>
      ) : rescoping && !hasCards ? (
        <CreatorHubChrome active="scenes" signedIn={isConnected} account={address ?? ""} name={name} onSignIn={onSignIn}>
          <div className="create-scenes__loading" style={SCENES_LOADING_WRAP} role="status" aria-live="polite">
            <span className="u-spinner" aria-hidden="true" />
            <p style={SCENES_LOADING_TEXT}>Loading your scenes&#x2026;</p>
          </div>
        </CreatorHubChrome>
      ) : !hasCards ? (
        <ChScenesEmptyState
          signedIn={isConnected}
          account={address ?? ""}
          name={name}
          onSignIn={onSignIn}
          onCreateScene={onCreateScene}
          onImport={onImport}
          onTemplates={onTemplates}
        />
      ) : (
        <ChScenes
          state="default"
          signedIn={isConnected}
          account={address ?? ""}
          name={name}
          onSignIn={onSignIn}
          projects={mergedCards}
          onSelectScene={onSelectScene}
          onCreateScene={onCreateScene}
          onTemplates={onTemplates}
          onImport={onImport}
          onDeleteScene={onDeleteScene}
          tabs={tabs}
          onTab={onTab}
        />
      )}
      {pendingDelete ? (
        <ChModalDeleteProject
          key={pendingDelete.slug}
          open
          chrome={false}
          project={{
            id: `local:${pendingDelete.slug}`,
            title: pendingDelete.title,
            path: "",
          }}
          deleteFiles={false}
          showFilesOption={pendingDelete.hasFolder}
          note={
            pendingDelete.hasFolder
              ? undefined
              : "This scene isn't connected to a folder on your computer \u{2014} deleting it from My Scenes permanently removes its saved content."
          }
          onClose={onCancelLocalDelete}
          onSubmit={(_project, shouldDeleteFiles) => {
            void onConfirmLocalDelete(shouldDeleteFiles);
          }}
          onToggleFiles={(checked) =>
            track(
              "ch_delete_files_toggled",
              { checked, project_id: `local:${pendingDelete.slug}`, local: true },
              { sid, story: STORY },
            )
          }
        />
      ) : null}
      {deleteNotice ? (
        <div role="status" aria-live="polite" style={SCENES_DELETE_TOAST}>
          {deleteNotice}
        </div>
      ) : null}
      {importPhase === "reading" || importPhase === "opening" ? (
        <div role="status" aria-live="polite" style={SCENES_IMPORT_PENDING_TOAST}>
          <span className="u-spinner" aria-hidden="true" />
          <span>
            {importPhase === "reading" ? "Reading scene folder\u{2026}" : "Opening scene\u{2026}"}
          </span>
        </div>
      ) : null}
      {importError ? (
        <div role="alert" style={SCENES_IMPORT_ERROR_TOAST}>
          <span>{importError}</span>
          <button
            type="button"
            onClick={() => setImportError(null)}
            style={SCENES_IMPORT_ERROR_DISMISS}
          >
            Dismiss
          </button>
        </div>
      ) : null}
    </div>
  );
}

const SCENES_DELETE_TOAST: React.CSSProperties = {
  position: "fixed",
  left: "50%",
  bottom: 24,
  transform: "translateX(-50%)",
  maxWidth: 560,
  background: "var(--panel, #1a1a1a)",
  border: "1px solid var(--line)",
  color: "var(--ink-85, #fff)",
  borderRadius: "var(--r-control, 8px)",
  padding: "12px 16px",
  fontSize: 13,
  lineHeight: 1.5,
  zIndex: 900,
  boxShadow: "var(--shadow-modal)",
};

const SCENES_IMPORT_PENDING_TOAST: React.CSSProperties = {
  ...SCENES_DELETE_TOAST,
  display: "flex",
  alignItems: "center",
  gap: 10,
};

const SCENES_IMPORT_ERROR_TOAST: React.CSSProperties = {
  ...SCENES_DELETE_TOAST,
  display: "flex",
  alignItems: "flex-start",
  gap: 12,
  border: "1px solid color-mix(in srgb, var(--error) 45%, transparent)",
  background: "color-mix(in srgb, var(--error) 16%, var(--panel, #1a1a1a))",
};

const SCENES_IMPORT_ERROR_DISMISS: React.CSSProperties = {
  flex: "0 0 auto",
  border: "1px solid color-mix(in srgb, var(--error) 45%, transparent)",
  background: "transparent",
  color: "inherit",
  borderRadius: "var(--r-control, 8px)",
  padding: "4px 10px",
  fontSize: 12,
  fontWeight: 600,
  cursor: "pointer",
};

const SCENES_ERROR_WRAP: React.CSSProperties = { padding: 24 };
const SCENES_ERROR: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 16,
  flexWrap: "wrap",
  border: "1px solid color-mix(in srgb, var(--error) 35%, transparent)",
  background: "color-mix(in srgb, var(--error) 12%, transparent)",
  color: "color-mix(in srgb, var(--error) 45%, var(--text))",
  borderRadius: "var(--r-card)",
  padding: "12px 16px",
  fontSize: 14,
  lineHeight: 1.5,
};
const SCENES_LOADING_WRAP: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "center",
  gap: 14,
  minHeight: 360,
  padding: 24,
};
const SCENES_LOADING_TEXT: React.CSSProperties = {
  margin: 0,
  color: "var(--ink-45)",
  fontSize: 14,
  lineHeight: 1.5,
};
const SCENES_RETRY: React.CSSProperties = {
  flex: "0 0 auto",
  border: "1px solid color-mix(in srgb, var(--error) 45%, transparent)",
  background: "color-mix(in srgb, var(--error) 16%, transparent)",
  color: "inherit",
  borderRadius: "var(--r-control)",
  padding: "6px 14px",
  fontSize: 13,
  fontWeight: 600,
  cursor: "pointer",
};
