import { useCallback, useEffect, useRef, useState } from "react";
import { data, useSearchParams } from "react-router";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import SceneAnalyticsView from "@ui/creatorhub/pages/SceneAnalyticsView";
import type {
  SceneSelection,
} from "@ui/creatorhub/pages/SceneAnalyticsView";
import type {
  CreatorScenesStats,
  SceneStats,
  SceneType,
} from "@ui/creatorhub/lib/scene-analytics";
import { sceneKey } from "@ui/creatorhub/lib/scene-analytics";

import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { fetchCreatorScenesStats } from "@data/lib/catalyst/creator-hub/scene-analytics";
import { getJSON, worldsBase } from "@data/lib/catalyst/client";
import { ensureSid, serializeSidCookie } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";
import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

import type { Route } from "./+types/creator-hub.scene-analytics";

export const meta = () => creatorHubMeta("Scene Analytics");

const STORY = "creator-hub/scene-analytics";
const SCENE_ANALYTICS_VIEWED = "scene_analytics_viewed";

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, created } = ensureSid(request);
  const payload = { sid };
  return created
    ? data(payload, { headers: { "Set-Cookie": serializeSidCookie(sid) } })
    : payload;
}

function parseSelection(raw: string | null): SceneSelection | null {
  if (!raw) return null;
  const split = raw.indexOf(":");
  if (split <= 0) return null;
  const sceneType = raw.slice(0, split);
  const sceneId = raw.slice(split + 1);
  if ((sceneType !== "genesis" && sceneType !== "world") || !sceneId)
    return null;
  return { sceneType: sceneType as SceneType, sceneId };
}

type FetchStatus = "idle" | "loading" | "succeeded" | "failed";

export default function CreatorHubSceneAnalyticsRoute({
  loaderData,
}: Route.ComponentProps) {
  const { sid } = loaderData as { sid: string };
  const { isConnected, address, identity } = useAuth();
  const name = useProfileName(address, isConnected);
  const [searchParams, setSearchParams] = useSearchParams();

  const selected = parseSelection(searchParams.get("scene"));

  const [status, setStatus] = useState<FetchStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<CreatorScenesStats | null>(null);
  const [worldAccess, setWorldAccess] = useState<
    Record<string, "public" | "private">
  >({});

  const viewedRef = useRef(false);
  useEffect(() => {
    if (viewedRef.current) return;
    viewedRef.current = true;
    track(
      SCENE_ANALYTICS_VIEWED,
      {
        source: searchParams.get("src") ?? "direct",
        scene_type: selected?.sceneType,
        scene_id: selected?.sceneId,
      },
      { sid, story: STORY },
    );
  }, []);

  useEffect(() => {
    if (!identity || status !== "idle") return;
    let cancelled = false;
    setStatus("loading");
    setError(null);
    fetchCreatorScenesStats(identity)
      .then((next) => {
        if (cancelled) return;
        setStats(next);
        setStatus("succeeded");
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(
          err instanceof Error ? err.message : "Failed to fetch scene metrics",
        );
        setStatus("failed");
      });
    return () => {
      cancelled = true;
    };
  }, [identity, status]);

  const selectedWorld =
    selected?.sceneType === "world" ? selected.sceneId : null;
  useEffect(() => {
    if (!selectedWorld || worldAccess[selectedWorld]) return;
    let cancelled = false;
    getJSON<{ permissions?: { access?: { type?: string } } }>(
      `/world/${encodeURIComponent(selectedWorld)}/permissions`,
      { base: worldsBase() },
    )
      .then((response) => {
        const type = response?.permissions?.access?.type;
        if (cancelled || !type) return;
        setWorldAccess((current) => ({
          ...current,
          [selectedWorld]: type === "unrestricted" ? "public" : "private",
        }));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [selectedWorld, worldAccess]);

  const handleOpenScene = useCallback(
    (scene: SceneStats) => {
      setSearchParams(
        (current) => {
          const next = new URLSearchParams(current);
          next.set("scene", sceneKey(scene));
          next.delete("src");
          return next;
        },
        { preventScrollReset: true },
      );
    },
    [setSearchParams],
  );

  const handleBack = useCallback(() => {
    setSearchParams(
      (current) => {
        const next = new URLSearchParams(current);
        next.delete("scene");
        next.delete("src");
        return next;
      },
      { preventScrollReset: true },
    );
  }, [setSearchParams]);

  const handleRetry = useCallback(() => {
    setStatus("idle");
  }, []);

  const handleExportCsv = useCallback(
    (scene: SceneStats) => {
      track(
        "scene_analytics_exported",
        { scene_type: scene.sceneType, scene_id: scene.sceneId },
        { sid, story: STORY },
      );
    },
    [sid],
  );

  const phase = !isConnected
    ? "signed-out"
    : status === "failed"
      ? "error"
      : status === "succeeded"
        ? "ready"
        : "loading";

  return (
    <CreatorHubChrome
      active="metrics"
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={() => {
        openSignIn();
      }}
    >
      <SceneAnalyticsView
        phase={phase}
        error={error}
        scenes={stats?.scenes ?? []}
        asOf={stats?.asOf ?? null}
        selected={selected}
        worldAccess={worldAccess}
        onConnect={() => openSignIn()}
        onRetry={handleRetry}
        onOpenScene={handleOpenScene}
        onBack={handleBack}
        onExportCsv={handleExportCsv}
      />
    </CreatorHubChrome>
  );
}
