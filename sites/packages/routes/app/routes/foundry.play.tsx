import { useEffect, useRef } from "react";

import type { FdGameCardVM } from "@ui/foundry/components/FdGameCard";
import { runVerdictReading } from "@ui/foundry/components/FdVerdictPill";
import FdPlayPage from "@ui/foundry/pages/FdPlayPage";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdgamecard.css";
import "@ui/foundry/components/fdcellchip.css";
import "@ui/foundry/components/fdverdictpill.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { benchSummaryByScene } from "@data/lib/foundry/bench.server";
import { activeStewardsByScene } from "@data/lib/foundry/continuity.server";
import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { worldLink } from "@data/lib/foundry/play.server";
import { activeRoles } from "@data/lib/foundry/roles.server";
import { lastMovedByScene, listScenes } from "@data/lib/foundry/scenes.server";

import type { Route } from "./+types/foundry.play";

export function meta() {
  return [{ title: "Games — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);

  let games: FdGameCardVM[] = [];
  let unavailable = false;
  let canHost = false;

  try {
    const db = getPool();
    const [scenes, bench, stewards, moved, roles] = await Promise.all([
      listScenes(db),
      benchSummaryByScene(db),
      activeStewardsByScene(),
      lastMovedByScene(db),
      activeRoles(sid),
    ]);
    canHost = roles.includes("host");
    const byScene = new Map(bench.map((b) => [b.sceneId, b]));
    games = scenes.map((scene) => {
      const summary = byScene.get(scene.id);
      const reading = summary?.lastVerdict
        ? runVerdictReading({
            verdict: summary.lastVerdict,
            checksFailed: summary.lastChecksFailed,
            checksTotal: summary.lastChecksTotal,
            checksUnevaluable: summary.lastChecksUnevaluable,
          })
        : null;
      const when =
        summary?.lastRealRanAt ? ` on ${summary.lastRealRanAt.slice(0, 10)}` : "";
      return {
        slug: scene.id,
        title: scene.title,
        worldName: scene.worldName,
        deployedAt: scene.deployedAt,
        importedAt: scene.importedAt,
        sizeBytes: scene.sizeBytes,
        parcels: scene.parcels,
        sourceNote: scene.sourceNote,
        description: scene.description,
        stewardName: stewards.get(scene.id) ?? null,
        lastMovedAt: (() => {
          const recorded = moved.get(scene.id) ?? null;
          if (recorded === null) return null;
          return scene.deployedAt !== null && recorded <= scene.deployedAt
            ? null
            : recorded;
        })(),
        thumbnailUrl: scene.thumbnailUrl,
        verdict: reading?.verdict ?? null,
        verdictLabel: reading?.label ?? null,
        verdictDetail: reading?.detail ? reading.detail + when : null,
        benchRuns: summary?.runs ?? 0,
        sandboxRuns: summary?.sandboxRuns ?? 0,
        href: `/foundry/play/${scene.id}`,
        playHref: worldLink(scene),
        gddHref: scene.gddDocId ? `/foundry/gdd/${scene.gddDocId}` : null,
        // This program's own market-cell reading, carried as what it is; a
        // scene never read stays null and renders no chip.
        cell: scene.marketCell,
      };
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    unavailable = true;
  }

  return wrap({ badge: sidBadge(sid), unavailable, games, canHost });
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
      <div className="fd-page fd-stack">
        <FdPageHead title="The games" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return (
    <FdPlayPage
      games={d.games}
      registerHref={d.canHost ? "/foundry/play/register" : null}
      onLinkOpen={(slug, target) =>
        track("fd_game_link_opened", { slug, target }, ctx)
      }
    />
  );
}
