import { useEffect } from "react";
import type { ReactNode } from "react";
import { Link, useSearchParams } from "react-router";
import { href } from "@core/lib/router/routes";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import CreatorHubBreadcrumb from "@ui/creatorhub/components/CreatorHubBreadcrumb";
import "@ui/creatorhub/frames/creatorhubchrome.css";
import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import {
  loadWorldPermissions,
  type LoadWorldPermissionsResult,
} from "@data/lib/catalyst/creator-hub/world-permissions.server";
import { emptyWorldPermissions, viewerWorldRole, worldExists } from "@data/lib/catalyst/creator-hub/world-permissions";
import { loadWorldScenes } from "@data/lib/catalyst/creator-hub/world-scenes.server";
import { readWallet } from "@data/lib/auth/wallet-cookie";
import WorldPermissionsWizard from "@features/stories/creator-hub/world-permissions/WorldPermissionsWizard";
import type { CommitFn } from "@features/stories/creator-hub/world-permissions/machine";
import { commitWorldAccess } from "@data/lib/catalyst/creator-hub/world-permissions-commit";
import {
  WorldGateView,
  type WorldGate,
} from "@features/components/creator-hub/world-gate";

import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.world-permissions";
import type { StoryId } from "@core/lib/telemetry/story-id";

export const meta = () => creatorHubMeta("World permissions");

const STORY: StoryId = "creator-hub/world-permissions";

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true },
  experimentKey: "ch_world_perms_wizard",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;
  const worldName = url.searchParams.get("world")?.trim() || "";
  const noWorld = worldName === "";

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const viewer = (
    url.searchParams.get("address")?.trim() ||
    readWallet(request) ||
    ""
  ).toLowerCase();

  let perms: LoadWorldPermissionsResult | null = null;
  let gate: WorldGate = noWorld ? "no-world" : "none";
  if (!noWorld) {
    const [loaded, scenes] = await Promise.all([
      loadWorldPermissions(worldName, { signal: request.signal }).catch(
        () => null,
      ),
      loadWorldScenes(worldName, { signal: request.signal }),
    ]);
    perms =
      loaded ?? {
        permissions: emptyWorldPermissions(worldName),
        source: "empty",
        fallback: true,
      };
    if (perms.fallback || scenes === null) {
      gate = "load-failed";
    } else if (!worldExists(perms.permissions, scenes.length)) {
      gate = "not-found";
    } else if (viewerWorldRole(perms.permissions, viewer) === "none") {
      gate = "no-access";
    }
  }

  const payload = {
    sid,
    step,
    worldName,
    noWorld,
    gate,
    viewer,
    assignment,
    permissions: perms?.permissions ?? null,
    source: perms?.source ?? null,
    fallback: perms?.fallback ?? false,
  };
  return wrap(payload);
}

export default function CreatorHubWorldPermissions({
  loaderData,
}: Route.ComponentProps) {
  const { sid, step, assignment, permissions, gate, viewer, worldName } =
    loaderData;
  const { isConnected, address, identity } = useAuth();
  const name = useProfileName(address, isConnected);
  const commit: CommitFn | undefined =
    identity && worldName
      ? async ({ accessType, collaborators, signal }) => {
          await commitWorldAccess(
            worldName,
            { accessType, collaborators },
            { identity, signal },
          );
          return { aclWritten: true, addresses: collaborators.length };
        }
      : undefined;
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
  if (gate !== "none" || !permissions || !commit) {
    body = (
      <WorldGateView
        gate={gate === "none" ? "no-world" : gate}
        worldName={worldName}
        surface="permissions"
        checkingAccess={rescoping || (gate === "none" && !!permissions && !commit)}
        signedIn={isConnected}
        onSignIn={() => openSignIn()}
      />
    );
  } else {
    body = (
      <WorldPermissionsWizard
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        data={permissions}
        initialStep={step ?? undefined}
        commit={commit}
      />
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

      <section className="creator-hub-world-permissions">{body}</section>
    </CreatorHubChrome>
  );
}
