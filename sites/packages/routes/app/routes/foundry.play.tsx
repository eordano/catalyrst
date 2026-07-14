import { useEffect, useRef } from "react";

import EmptyState from "@ui/components/EmptyState";
import type { FdGameCardVM } from "@ui/foundry/components/FdGameCard";
import FdPlayPage from "@ui/foundry/pages/FdPlayPage";

import "@ui/components/emptystate.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdgamecard.css";
import "@ui/foundry/components/fdverdictpill.css";
import "@ui/foundry/pages/fdplay.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { benchSummaryByScene } from "@data/lib/foundry/bench.server";
import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { editorUrl, worldLink } from "@data/lib/foundry/play.server";
import { listScenes } from "@data/lib/foundry/scenes.server";

import type { Route } from "./+types/foundry.play";

const TEMPLATE_NOTE =
  "An SDK7 starter scene living in this repo — open it in the editor.";

export function meta() {
  return [{ title: "Games — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let games: FdGameCardVM[] = [];
  let unavailable = false;

  try {
    const db = getPool();
    const [scenes, bench] = await Promise.all([listScenes(db), benchSummaryByScene(db)]);
    const byScene = new Map(bench.map((b) => [b.sceneId, b]));
    games = scenes.map((scene) => {
      const summary = byScene.get(scene.id);
      return {
        slug: scene.id,
        title: scene.title,
        worldName: scene.worldName,
        deployedAt: scene.deployedAt,
        importedAt: scene.importedAt,
        sizeBytes: scene.sizeBytes,
        parcels: scene.parcels,
        source: scene.source,
        sourceNote: scene.sourceNote,
        verdict: summary?.lastVerdict ?? null,
        benchRuns: summary?.runs ?? 0,
        href: `/foundry/play/${scene.id}`,
        playHref: worldLink(scene),
        editorHref: editorUrl(scene),
        gddHref: scene.gddDocId ? `/foundry/gdd/${scene.gddDocId}` : null,
        note: scene.source === "repo" ? TEMPLATE_NOTE : null,
      };
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({ badge: sidBadge(sid), unavailable, games });
}

export default function FoundryPlay({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("fd_play_viewed", { scenes_total: d.games.length }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge]);

  if (d.unavailable) {
    return (
      <EmptyState
        variant="inline"
        title="The Foundry database is not configured"
        subtitle="The registry of games lives in Postgres. With no database behind it there is nothing to read, and nothing here was invented to fill the gap."
      />
    );
  }

  return (
    <FdPlayPage
      games={d.games}
      onLinkOpen={(slug, target) =>
        track("fd_game_link_opened", { slug, target }, ctx)
      }
    />
  );
}
