import { useEffect, useRef, useState } from "react";
import { data } from "react-router";

import EmptyState from "@ui/components/EmptyState";
import { evidenceLabel } from "@ui/foundry/components/evidence";
import type { FdBenchReportVM } from "@ui/foundry/pages/FdBenchPage";
import FdGameDetail from "@ui/foundry/pages/FdGameDetail";

import "@ui/atoms/button.css";
import "@ui/components/emptystate.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdembedviewport.css";
import "@ui/foundry/components/fdverdictpill.css";
import "@ui/foundry/pages/fdbench.css";
import "@ui/foundry/pages/fdgamedetail.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { countBenchReports, listBenchReports } from "@data/lib/foundry/bench.server";
import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { getGddDocSummary, type GddDocSummary } from "@data/lib/foundry/gdd.server";
import { editorUrl, sceneEmbed, worldLink } from "@data/lib/foundry/play.server";
import { getScene } from "@data/lib/foundry/scenes.server";
import type { BotReport } from "@data/lib/foundry/types";

import type { Route } from "./+types/foundry.play_.$slug";

// The embed is a cross-origin engine that needs shared memory: the parent page
// has to be cross-origin isolated for the iframe's `cross-origin-isolated`
// delegation to mean anything. Same pair the live scene editor sets.
export function headers(): Record<string, string> {
  return {
    "Cross-Origin-Opener-Policy": "same-origin",
    "Cross-Origin-Embedder-Policy": "credentialless",
  };
}

export function meta({ loaderData }: Route.MetaArgs) {
  const title = (loaderData as { game?: { title?: string } | null } | undefined)?.game
    ?.title;
  return [{ title: title ? `${title} — The Foundry` : "Game — The Foundry" }];
}

function toReportVM(report: BotReport): FdBenchReportVM {
  return {
    id: report.id,
    slug: report.slug,
    runner: report.runner,
    realm: report.realm,
    ranAt: report.ranAt,
    verdict: report.verdict,
    checksTotal: report.checksTotal,
    checksFailed: report.checksFailed,
    missingTools: report.missingTools,
    stubbedTools: report.stubbedTools,
    networkWrites: report.networkWrites,
    shots: report.shots.length,
    // Label only — the raw absolute evidence path never leaves the server.
    evidence: report.evidencePath ? evidenceLabel(report.evidencePath) : null,
    replayHref: report.trajectoryId
      ? `/foundry/console/trajectories/${report.trajectoryId}`
      : null,
    gameHref: null,
  };
}

export async function loader({ params, request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const slug = params.slug;

  try {
    const db = getPool();
    const scene = await getScene(db, slug);
    if (!scene) throw data(null, { status: 404 });

    const [embed, reports, reportsTotal, gdd] = await Promise.all([
      sceneEmbed(scene),
      listBenchReports(db, scene.id, 5),
      countBenchReports(db, scene.id),
      scene.gddDocId ? getGddDocSummary(db, scene.gddDocId) : null,
    ]);

    return wrap({
      badge: sidBadge(sid),
      unavailable: false,
      slug,
      game: {
        slug: scene.id,
        title: scene.title,
        worldName: scene.worldName,
        deployedAt: scene.deployedAt,
        importedAt: scene.importedAt,
        sizeBytes: scene.sizeBytes,
        parcels: scene.parcels,
        source: scene.source,
        sourceNote: scene.sourceNote,
        // Labels only — the raw absolute repo/manifest paths never leave the server.
        repoLabel: scene.repoPath ? evidenceLabel(scene.repoPath) : null,
        botManifestLabel: scene.botManifest ? evidenceLabel(scene.botManifest) : null,
      },
      embed,
      worldHref: worldLink(scene),
      editorHref: editorUrl(scene),
      gddHref: scene.gddDocId ? `/foundry/gdd/${scene.gddDocId}` : null,
      gdd,
      reports: reports.map(toReportVM),
      reportsTotal,
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    return wrap({
      badge: sidBadge(sid),
      unavailable: true,
      slug,
      game: null,
      embed: null,
      worldHref: null,
      editorHref: "",
      gddHref: null,
      gdd: null as GddDocSummary | null,
      reports: [] as FdBenchReportVM[],
      reportsTotal: 0,
    });
  }
}

export default function FoundryGame({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };
  const [embedStarted, setEmbedStarted] = useState(false);

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || !d.game) return;
    viewed.current = true;
    track("fd_game_viewed", { slug: d.slug }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge, d.slug]);

  if (d.unavailable || !d.game) {
    return (
      <EmptyState
        variant="inline"
        title="The Foundry database is not configured"
        subtitle="This page reads one row of the game registry. With no database behind it there is nothing to read, and nothing here was invented to fill the gap."
      />
    );
  }

  return (
    <FdGameDetail
      game={d.game}
      embed={d.embed}
      worldHref={d.worldHref}
      editorHref={d.editorHref}
      gddHref={d.gddHref}
      gdd={d.gdd}
      reports={d.reports}
      reportsTotal={d.reportsTotal}
      embedStarted={embedStarted}
      onEmbedStart={() => {
        setEmbedStarted(true);
        track(
          "fd_embed_started",
          { slug: d.slug, reachable: d.embed?.reachable ?? false },
          ctx,
        );
      }}
      onLinkOpen={(target) => track("fd_game_link_opened", { slug: d.slug, target }, ctx)}
      onReportOpen={(report) =>
        track(
          "fd_bench_report_viewed",
          {
            slug: report.slug,
            // An arena run's verdict is a process exit code, not a test of the
            // game, and the card suppresses its pill. Report "none" rather than a
            // bare "pass" the reader never saw.
            verdict: report.runner === "arena" ? "none" : report.verdict ?? "none",
          },
          ctx,
        )
      }
    />
  );
}
