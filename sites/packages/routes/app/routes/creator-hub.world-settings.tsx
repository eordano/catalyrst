import { useEffect } from "react";
import type { ReactNode } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { href } from "@core/lib/router/routes";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import CreatorHubBreadcrumb from "@ui/creatorhub/components/CreatorHubBreadcrumb";
import "@ui/creatorhub/frames/creatorhubchrome.css";
import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useChromeAuth } from "@ui/web/frames/chrome-auth";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoaderWith } from "@core/lib/experiments/story-loader";

import { loadWorldSettings } from "@data/lib/catalyst/creator-hub/world-settings";
import { worldsBase } from "@data/lib/catalyst/client";
import { loadWorldPermissions } from "@data/lib/catalyst/creator-hub/world-permissions.server";
import {
  viewerWorldRole,
  worldExists,
} from "@data/lib/catalyst/creator-hub/world-permissions";
import { loadWorldScenes } from "@data/lib/catalyst/creator-hub/world-scenes.server";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import { fetchParcelsPermission } from "@data/lib/catalyst/creator-hub/unpublish-scene";
import WorldSettingsWizard, {
  type WorldSceneVM,
} from "@features/stories/creator-hub/world-settings/WorldSettingsWizard";
import {
  WorldGateView,
  type WorldGate,
} from "@features/components/creator-hub/world-gate";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.world-settings";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("World settings");

const STORY: StoryId = "creator-hub/world-settings";

function tabToStep(tab: string): string | null {
  if (tab === "layout") return "layout";
  if (tab === "details") return "details";
  if (tab === "general" || tab === "misc") return "misc";
  return null;
}

async function loadWorldSettingsScenes(
  worldName: string,
  viewer: string,
  signal?: AbortSignal,
): Promise<{ scenes: WorldSceneVM[]; isOwner: boolean; gate: WorldGate }> {
  const [backendScenes, perms] = await Promise.all([
    loadWorldScenes(worldName, { signal }),
    loadWorldPermissions(worldName, { signal }).catch(() => null),
  ]);

  if (!perms || perms.fallback || backendScenes === null) {
    return { scenes: [], isOwner: false, gate: "load-failed" };
  }
  if (!worldExists(perms.permissions, backendScenes.length)) {
    return { scenes: [], isOwner: false, gate: "not-found" };
  }
  const role = viewerWorldRole(perms.permissions, viewer);
  if (role === "none") {
    return { scenes: [], isOwner: false, gate: "no-access" };
  }

  const isOwner = role === "owner";
  const deployWallets = perms.permissions.permissions.deployment.wallets.map((w) =>
    w.toLowerCase(),
  );
  const isDeployer = viewer !== "" && deployWallets.includes(viewer);
  const parcelPermission = isDeployer
    ? await fetchParcelsPermission(worldName, viewer, { base: worldsBase(), signal })
    : { parcels: [], total: 0 };
  if (parcelPermission === null) {
    return { scenes: [], isOwner: false, gate: "load-failed" };
  }
  const permittedParcels = parcelPermission.parcels;
  const worldWideDeployment = isDeployer && permittedParcels.length === 0;
  const permitted = new Set(permittedParcels);

  const scenes: WorldSceneVM[] = backendScenes.map((s) => ({
    entityId: s.entityId,
    title: `Scene ${s.baseParcel}`,
    coord: s.baseParcel,
    parcelCount: s.parcels.length,
    canUnpublish:
      isOwner || worldWideDeployment || s.parcels.some((p) => permitted.has(p)),
  }));

  return { scenes, isOwner, gate: "none" };
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "ch_world_settings_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const tabParam = url.searchParams.get("tab")?.trim() || "";
  const step =
    (url.searchParams.get("step")?.trim() ||
      (tabParam ? tabToStep(tabParam) : null)) ||
    null;

  const worldName = url.searchParams.get("world")?.trim() || "";
  const noWorld = worldName === "";
  const viewer = (
    url.searchParams.get("address")?.trim() ||
    readWallet(request) ||
    ""
  ).toLowerCase();

  const { sid, assignment, wrap, data } = await storyLoaderWith(
    request,
    STORY,
    FALLBACK,
    async () => {
      let scenes: WorldSceneVM[] = [];
      let isOwner = false;
      let gate: WorldGate = noWorld ? "no-world" : "none";
      if (!noWorld) {
        const loaded = await loadWorldSettingsScenes(
          worldName,
          viewer,
          request.signal,
        ).catch(() => ({
          scenes: [] as WorldSceneVM[],
          isOwner: false,
          gate: "load-failed" as const,
        }));
        scenes = loaded.scenes;
        isOwner = loaded.isOwner;
        gate = loaded.gate;
      }
      return { scenes, isOwner, gate };
    },
  );
  const { scenes, isOwner } = data;
  let gate = data.gate;
  const settings = gate === "none" && isOwner
    ? await loadWorldSettings(worldName, { signal: request.signal }).catch(() => null)
    : null;
  if (gate === "none" && isOwner && !settings) gate = "load-failed";

  const payload = {
    sid,
    step,
    assignment,
    worldName,
    noWorld,
    gate,
    viewer,
    scenes,
    isOwner,
    settings,
  };
  return wrap(payload);
}

export default function CreatorHubWorldSettings({ loaderData }: Route.ComponentProps) {
  const { sid, step, assignment, worldName, gate, viewer, scenes, isOwner, settings } =
    loaderData;
  const { isConnected, address } = useAuth();
  const { name } = useChromeAuth();
  const navigate = useNavigate();
  const [, setSearchParams] = useSearchParams();

  const rescoping = isConnected && Boolean(address) && viewer === "" && gate !== "no-world";
  useEffect(() => {
    if (isConnected && address && viewer === "" && gate !== "no-world") {
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("address", address);
          return next;
        },
        { replace: true, preventScrollReset: true },
      );
    }
  }, [isConnected, address, viewer, gate, setSearchParams]);

  let body: ReactNode;
  if (gate !== "none") {
    body = (
      <WorldGateView
        gate={gate}
        worldName={worldName}
        surface="settings"
        checkingAccess={rescoping}
        signedIn={isConnected}
        onSignIn={() => openSignIn()}
      />
    );
  } else {
    const viewerSuffix = viewer ? `&address=${encodeURIComponent(viewer)}` : "";
    body = (
      <>
        <nav
          aria-label="World tools"
          style={{ display: "flex", gap: 12, marginBottom: 14, flexWrap: "wrap" }}
        >
          <Link
            to={`/creator-hub/world-permissions?world=${encodeURIComponent(worldName)}${viewerSuffix}`}
            prefetch="intent"
            className="es__cta es__cta--outline"
          >
            Permissions
          </Link>
          <Link
            to={`/creator-hub/worlds-storage?world=${encodeURIComponent(worldName)}${viewerSuffix}`}
            prefetch="intent"
            className="es__cta es__cta--outline"
          >
            Storage
          </Link>
        </nav>


        <WorldSettingsWizard
          trackCtx={{
            sid,
            story: STORY,
            variant: assignment.variant,
            experimentKey: assignment.experimentKey,
          }}
          key={`${worldName}:${address ?? ""}`}
          worldName={worldName}
          settings={settings ?? { title: "", description: "", categories: [], spawnCoordinates: "0,0", skyboxTime: null, singlePlayer: false, showInPlaces: true, thumbnailUrl: null }}
          scenes={scenes}
          isOwner={isOwner && address?.toLowerCase() === viewer.toLowerCase()}
          initialStep={step ?? undefined}
          onClose={() => navigate("/creator-hub/manage")}
        />
      </>
    );
  }

  return (
    <CreatorHubChrome
      active="manage"
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => openSignIn()}
    >
      <CreatorHubBreadcrumb to={href("/creator-hub/manage")} label="Back to Worlds" LinkComponent={Link} />

      <section className="creator-hub-world-settings">{body}</section>
    </CreatorHubChrome>
  );
}
