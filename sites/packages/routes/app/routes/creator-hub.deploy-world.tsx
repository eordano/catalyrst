import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Link, useNavigate, useSearchParams } from "react-router";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import "@ui/creatorhub/frames/creatorhubchrome.css";
import DeployTargetHealthView from "@ui/creatorhub/components/DeployTargetHealthView";
import CreatorHubBreadcrumb from "@ui/creatorhub/components/CreatorHubBreadcrumb";
import ChPublishWizardPublishToWorld from "@ui/creatorhub/workflows/ChPublishWizardPublishToWorld";

import { resolveBreadcrumbOrigin, type BreadcrumbOrigin } from "@features/components/creator-hub/breadcrumbOrigins";
import { loadDeployWorld } from "@data/lib/catalyst/creator-hub/deploy-world.server";
import {
  landJumpUrl,
  shortAddress,
  worldJumpUrl,
  type DeployFile,
  type DeployWorldData,
} from "@data/lib/catalyst/creator-hub/deploy-world";
import {
  deployLandScene,
  deployWorldScene,
  type DeployContentFile,
  type WorldSceneMetadata,
} from "@data/lib/catalyst/creator-hub/deploy-scene";
import {
  buildDraftDeployFiles,
  fetchGameRuntime,
  packageSceneAssets,
  sceneMainOf,
  EMPTY_SRC_ERROR,
  RUNTIME_NOTE,
  type DraftPackage,
} from "@data/lib/catalyst/creator-hub/deploy-packaging";
import {
  parseParcel,
  probeLandRights,
  type LandRights,
} from "@data/lib/catalyst/creator-hub/land-rights";
import { openDirectory, readDirectoryFiles } from "@data/lib/fs/disk";
import {
  snapshotProjectDir,
  type ProjectSnapshot,
} from "@data/lib/fs/project-snapshot";
import { handleStore, ensureHandlePermission } from "@data/lib/fs/handle-store";
import { getIdentity, useAuth } from "@data/lib/auth/index";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import DeployWorldWizard, {
  claimNameUrl,
} from "@features/stories/creator-hub/deploy-scene/DeployWorldWizard";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.deploy-world";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("Publish");

const STORY: StoryId = "creator-hub/deploy-scene";

const CLAIM_TEST_MODE_NOTE =
  "NAME registration isn't connected on this realm yet \u{2014} no real NAME can be " +
  "bought here. You can publish to a NAME you already own.";

async function reuseProjectDir(): Promise<FileSystemDirectoryHandle | null> {
  if (typeof window === "undefined") return null;
  const slug = new URLSearchParams(window.location.search).get("project")?.trim();
  if (!slug) return null;
  try {
    const handle = await handleStore.get(slug);
    if (!handle) return null;
    return (await ensureHandlePermission(handle, "read")) ? handle : null;
  } catch {
    return null;
  }
}

async function hasGrantedReadPermission(
  handle: FileSystemDirectoryHandle,
): Promise<boolean> {
  const h = handle as unknown as {
    queryPermission?: (d: { mode: "read" | "readwrite" }) => Promise<PermissionState>;
  };
  try {
    if (typeof h.queryPermission === "function") {
      return (await h.queryPermission({ mode: "read" })) === "granted";
    }
  } catch {
    return false;
  }
  return true;
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "ch_deploy_world_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const address = url.searchParams.get("address")?.trim() || readWallet(request);
  const forceEmpty = url.searchParams.get("empty") === "1";
  const from = url.searchParams.get("from")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let deployData: DeployWorldData;
  try {
    deployData = await loadDeployWorld(address, { signal: request.signal });
  } catch {
    deployData = {
      address: address ?? "",
      names: [],
      liveEmpty: true,
      worldsOnline: null,
      project: { title: "Your scene", size: "", grad: "#222" },
      files: [],
      maxFileSizeMb: 50,
      owner: {
        network: "Mainnet",
        address: address ? shortAddress(address) : "0x\u{2026}",
        username: "",
        verified: false,
        role: "Owner",
      },
      source: "empty",
    };
  }
  const namesEmpty = forceEmpty || deployData.liveEmpty;

  const payload = {
    sid,
    step,
    from,
    assignment,
    deploy: deployData,
    namesEmpty,
    signedOut: !address,
  };
  return wrap(payload);
}

export default function CreatorHubDeployWorld({ loaderData }: Route.ComponentProps) {
  const { sid, step, from, assignment, deploy, namesEmpty, signedOut } =
    loaderData;

  const navigate = useNavigate();

  const { address: walletAddress, isConnected } = useAuth();
  const name = useProfileName(walletAddress, isConnected);
  const [searchParams, setSearchParams] = useSearchParams();
  const origin = useMemo<BreadcrumbOrigin>(() => {
    if (from !== "scene-editor") return resolveBreadcrumbOrigin(from);
    const slug = searchParams.get("project")?.trim();
    const draft = searchParams.get("draft")?.trim();
    const params = new URLSearchParams();
    if (slug) {
      params.set("source", "local");
      params.set("project", slug);
    }
    if (draft) params.set("draft", draft);
    if (!slug && !draft) params.set("new", "1");
    params.set("from", "scenes");
    return {
      active: "scenes",
      label: "Back to editor",
      to: `/creator-hub/scene-editor?${params.toString()}`,
    };
  }, [from, searchParams]);
  useEffect(() => {
    if (searchParams.get("address")?.trim()) return;
    if (!isConnected || !walletAddress) return;
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("address", walletAddress.toLowerCase());
        return next;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [walletAddress, isConnected, searchParams, setSearchParams]);

  const [localProject, setLocalProject] = useState<{
    files: DeployFile[];
    project: DeployWorldData["project"];
    parcels: string[];
    baseParcel: string | null;
    mainPresent: boolean | null;
  } | null>(null);
  const [snapshotSettled, setSnapshotSettled] = useState(false);
  const dirRef = useRef<FileSystemDirectoryHandle | null>(null);
  const [draftPack, setDraftPack] = useState<
    (DraftPackage & { list: DeployFile[]; project: DeployWorldData["project"] }) | null
  >(null);

  const projectCardFrom = useCallback(
    (snap: ProjectSnapshot, fallbackTitle?: string): DeployWorldData["project"] => ({
      title: snap.title ?? fallbackTitle ?? deploy.project.title,
      size: snap.sceneSize ?? "\u{2014}",
      grad: deploy.project.grad,
    }),
    [deploy.project],
  );

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const slug = new URLSearchParams(window.location.search)
          .get("project")
          ?.trim();
        if (!slug) return;
        const meta = await handleStore.getMeta(slug).catch(() => null);
        const handle = await handleStore.get(slug).catch(() => null);
        if (cancelled || !handle) return;
        if (!(await hasGrantedReadPermission(handle))) return;
        const snap = await snapshotProjectDir(handle);
        if (cancelled) return;
        dirRef.current = handle;
        setLocalProject({
          files: snap.files,
          project: projectCardFrom(snap, meta?.title),
          parcels: snap.parcels,
          baseParcel: snap.baseParcel,
          mainPresent: snap.mainPresent,
        });
      } catch {
      } finally {
        if (!cancelled) setSnapshotSettled(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [projectCardFrom]);

  useEffect(() => {
    if (!snapshotSettled || localProject) return;
    let cancelled = false;
    void (async () => {
      try {
        const q = new URLSearchParams(window.location.search);
        const slug = q.get("draft")?.trim() || q.get("project")?.trim();
        if (!slug) return;
        const meta = await handleStore.getMeta(slug).catch(() => null);
        if (!meta || cancelled) return;
        const pack = await buildDraftDeployFiles(meta);
        if (cancelled) return;
        const list: DeployFile[] = pack.files
          .map((f) => ({ name: f.file, size: f.content.byteLength }))
          .sort((a, b) => a.name.localeCompare(b.name));
        setDraftPack({
          ...pack,
          list,
          project: {
            title: meta.title || deploy.project.title,
            size: "1x1",
            grad: deploy.project.grad,
          },
        });
      } catch {
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [snapshotSettled, localProject, deploy.project]);

  const prepareReview = useCallback(async (): Promise<boolean> => {
    if (localProject || draftPack) return true;
    const dir =
      dirRef.current ?? (await reuseProjectDir()) ?? (await openDirectory("read"));
    if (!dir) return false;
    try {
      const snap = await snapshotProjectDir(dir);
      dirRef.current = dir;
      setLocalProject({
        files: snap.files,
        project: projectCardFrom(snap),
        parcels: snap.parcels,
        baseParcel: snap.baseParcel,
        mainPresent: snap.mainPresent,
      });
      return true;
    } catch {
      return false;
    }
  }, [localProject, draftPack, projectCardFrom]);

  const [landProbe, setLandProbe] = useState<
    { status: "checking" } | { status: "none" } | LandRights
  >(() =>
    searchParams.get("project")?.trim() && !signedOut
      ? { status: "checking" }
      : { status: "none" },
  );
  const probeAddress = (walletAddress ?? deploy.address ?? "").trim().toLowerCase();
  const projectParcels = localProject?.parcels ?? [];
  const parcelsKey = projectParcels.join(";");

  useEffect(() => {
    if (!snapshotSettled) return;
    const parcels = parcelsKey ? parcelsKey.split(";") : [];
    if (!probeAddress || parcels.length === 0) {
      setLandProbe({ status: "none" });
      return;
    }
    let cancelled = false;
    setLandProbe({ status: "checking" });
    void Promise.race([
      probeLandRights(probeAddress, parcels),
      new Promise<LandRights>((resolve) =>
        setTimeout(() => resolve({ status: "unknown", parcel: parcels[0] }), 8000),
      ),
    ]).then((rights) => {
      if (!cancelled) setLandProbe(rights);
    });
    return () => {
      cancelled = true;
    };
  }, [snapshotSettled, probeAddress, parcelsKey]);

  const land =
    landProbe.status === "granted" && localProject
      ? {
          parcels: landProbe.parcels,
          baseParcel: localProject.baseParcel ?? landProbe.parcels[0] ?? "",
        }
      : null;

  const parcelsAreCoords =
    projectParcels.length > 0 && projectParcels.every((p) => parseParcel(p) !== null);
  let landNotice: string | undefined;
  if (landProbe.status === "denied") {
    landNotice = landProbe.owner
      ? `This scene sits on LAND ${landProbe.parcel}, owned by ${landProbe.owner} \u{2014} ` +
        "the signed-in wallet has no rights there, so publishing to that LAND is " +
        "unavailable. You can publish a copy to a World instead."
      : `The signed-in wallet has no rights on LAND ${landProbe.parcel}, so ` +
        "publishing to that LAND is unavailable. You can publish a copy to a World instead.";
  } else if (landProbe.status === "unknown" && parcelsAreCoords) {
    landNotice =
      `Couldn't verify your rights on LAND ${landProbe.parcel || projectParcels[0]} ` +
      "right now, so publishing to LAND stays off. You can publish a copy to a World instead.";
  }

  const realDeploy = useCallback(
    async ({
      name,
      target,
      signal,
    }: {
      name: string;
      target: "world" | "land";
      signal?: AbortSignal;
    }) => {
      const identity = getIdentity();
      if (!identity) throw new Error("Sign in to publish.");
      const dir =
        dirRef.current ??
        (await reuseProjectDir()) ??
        (draftPack ? null : await openDirectory("read"));

      let metadata: WorldSceneMetadata;
      let files: DeployContentFile[];
      if (!dir && draftPack) {
        if (draftPack.emptySrc > 0) {
          throw new Error(EMPTY_SRC_ERROR(draftPack.emptySrc));
        }
        if (draftPack.missing.length > 0) {
          throw new Error(
            `Cannot publish: missing assets: ${draftPack.missing.join(", ")}. Save ` +
              `the scene to a project folder so its assets are on disk, then publish from there.`,
          );
        }
        metadata = draftPack.metadata as unknown as WorldSceneMetadata;
        files = draftPack.files;
      } else {
        if (!dir) throw new Error("Select your scene's project folder to publish.");
        const map = await readDirectoryFiles(dir);
        if (!map["scene.json"]) {
          throw new Error("No scene.json found in the selected folder.");
        }

        metadata = JSON.parse(await map["scene.json"].text()) as WorldSceneMetadata;
        files = [];
        for (const [file, f] of Object.entries(map)) {
          files.push({ file, content: new Uint8Array(await f.arrayBuffer()) });
        }

        const compositeKey = Object.keys(map).find((k) =>
          /(^|\/)main\.composite$/i.test(k),
        );
        if (compositeKey) {
          let compositeText: string | null = null;
          try {
            compositeText = await map[compositeKey].text();
          } catch {
            console.warn(
              "[deploy] main.composite unreadable \u{2014} skipping asset validation",
            );
          }
          if (compositeText !== null) {
            const packaged = await packageSceneAssets({
              compositeText,
              fileKeys: Object.keys(map),
            });
            if (packaged.emptySrc > 0) {
              throw new Error(EMPTY_SRC_ERROR(packaged.emptySrc));
            }
            if (packaged.missing.length > 0) {
              throw new Error(
                `Cannot publish: missing assets: ${packaged.missing.join(", ")}. Re-open the scene and ` +
                  `Save so placed assets are written into the project folder before publishing.`,
              );
            }
            if (packaged.changed) {
              files = files.map((f) =>
                f.file === compositeKey
                  ? { file: f.file, content: new TextEncoder().encode(packaged.compositeText) }
                  : f,
              );
            }
            files.push(...packaged.extra);
          }
        }

        const main = sceneMainOf(metadata);
        if (!files.some((f) => f.file === main)) {
          const runtime = await fetchGameRuntime();
          if (!runtime) {
            throw new Error(
              `This project has no built ${main} and the prebuilt scene runtime is ` +
                `unavailable \u{2014} build the project locally (npm install && npm run build) before publishing.`,
            );
          }
          files.push({ file: main, content: runtime });
        }
      }
      if (target === "land") {
        const res = await deployLandScene(identity, { files, metadata }, { signal });
        if (!res.ok) {
          throw new Error(res.errors?.[0] ?? `Publish rejected (${res.status})`);
        }
        const scene = (metadata as { scene?: { base?: string; parcels?: string[] } })
          .scene;
        const base = scene?.base ?? scene?.parcels?.[0] ?? name;
        return { jumpUrl: landJumpUrl(base) };
      }
      const res = await deployWorldScene(
        identity,
        { worldName: name, files, metadata },
        { signal },
      );
      if (!res.ok) {
        throw new Error(res.errors?.[0] ?? `Publish rejected (${res.status})`);
      }
      return { jumpUrl: worldJumpUrl(name) };
    },
    [draftPack],
  );

  const wizardShownRef = useRef(false);
  if (landProbe.status !== "checking") wizardShownRef.current = true;

  return (
    <CreatorHubChrome
      active={origin.active}
      signedIn={isConnected}
      account={walletAddress ?? ""}
      name={name}
      onSignIn={() => openSignIn()}
    >
      <CreatorHubBreadcrumb to={origin.to} label={origin.label} LinkComponent={Link} />

      <section className="creator-hub-deploy-world">
        <DeployTargetHealthView online={deploy.worldsOnline} />

        {signedOut ? (
          <ChPublishWizardPublishToWorld
            state="signedOut"
            inline
            claimNote={CLAIM_TEST_MODE_NOTE}
            onSignIn={() => openSignIn()}
            onClaimName={() => navigate(claimNameUrl(searchParams))}
            onClose={() => {
              const idx =
                typeof window !== "undefined"
                  ? Number((window.history.state as { idx?: number } | null)?.idx ?? 0)
                  : 0;
              if (idx > 0) navigate(-1);
              else navigate(origin.to, { replace: true });
            }}
          />
        ) : !wizardShownRef.current ? (
          <p className="creator-hub-deploy-world__checking" role="status">
            Checking your LAND rights&#x2026;
          </p>
        ) : (
          <DeployWorldWizard
            key={`${deploy.address || "anon"}:${land ? "land" : "world"}`}
            trackCtx={{
              sid,
              story: STORY,
              variant: assignment.variant,
              experimentKey: assignment.experimentKey,
            }}
            names={deploy.names}
            namesEmpty={namesEmpty}
            claimNote={CLAIM_TEST_MODE_NOTE}
            land={land}
            landNotice={landNotice}
            files={localProject?.files ?? draftPack?.list ?? deploy.files}
            filesReady={!!localProject || !!draftPack}
            prepareReview={prepareReview}
            maxFileSizeMb={deploy.maxFileSizeMb}
            project={localProject?.project ?? draftPack?.project ?? deploy.project}
            owner={name ? { ...deploy.owner, username: name } : deploy.owner}
            runtimeNote={
              draftPack?.runtimeInjected || localProject?.mainPresent === false
                ? RUNTIME_NOTE
                : undefined
            }
            initialStep={step ?? undefined}
            closeTo={origin.to}
            deploy={realDeploy}
          />
        )}
      </section>
    </CreatorHubChrome>
  );
}
