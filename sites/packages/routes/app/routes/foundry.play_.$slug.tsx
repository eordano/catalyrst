import { useEffect, useRef, useState } from "react";
import { data, useFetcher } from "react-router";

import { evidenceLabel } from "@ui/foundry/components/evidence";
import FdContinuity, {
  type FdContinuitySummaryVM,
  type FdSceneMemoryRowVM,
  type FdStewardVM,
  type FdTransferVM,
} from "@ui/foundry/components/FdContinuity";
import type { FdBenchReportVM } from "@ui/foundry/pages/FdBenchPage";
import FdGameDetail, {
  type FdGameBenchOnlyRunVM,
  type FdGameBenchSummaryVM,
  type FdGameChangelogVM,
  type FdGameConceptReadingVM,
  type FdGameDesignVersionVM,
  type FdGameGatheringVM,
  type FdGameRunVM,
} from "@ui/foundry/pages/FdGameDetail";
import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/atoms/button.css";
import "@ui/components/emptystate.css";
import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdcellchip.css";
import "@ui/foundry/components/fdembedviewport.css";
import "@ui/foundry/components/fdhistoryspine.css";
import "@ui/foundry/components/fdverdictpill.css";
import "@ui/foundry/components/fdtable.css";
import "@ui/foundry/components/fdpersonachip.css";
import "@ui/foundry/components/fdcontinuity.css";
import "@ui/foundry/pages/fdbench.css";
import "@ui/foundry/pages/fdgamedetail.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import { benchChecklist, benchSummaryByScene, listBenchReports } from "@data/lib/foundry/bench.server";
import {
  addSceneNote,
  claimSteward,
  listSceneMemory,
  listStewards,
  listTransfers,
  offerTransfer,
  releaseSteward,
  revokeTransfer,
} from "@data/lib/foundry/continuity.server";
import { FoundryUnavailableError, getPool, sidBadge } from "@data/lib/foundry/db.server";
import { listSceneJobs } from "@data/lib/foundry/emotional-jobs.server";
import { reportShotHrefs } from "@data/lib/foundry/evidence.server";
import { continuitySummary } from "@data/lib/foundry/export.server";
import {
  getGddChain,
  getGddChainHead,
  readingForScene,
} from "@data/lib/foundry/gdd.server";
import { editorUrl, entityHref, sceneEmbed, worldLink } from "@data/lib/foundry/play.server";
import { listShelfAnswerAsks } from "@data/lib/foundry/request-readings.server";
import { getScene, listSceneChangelog } from "@data/lib/foundry/scenes.server";
import { listUpcoming } from "@data/lib/foundry/sessions.server";
import {
  countTrajectories,
  listTrajectories,
} from "@data/lib/foundry/trajectory.server";
import type { BotReport } from "@data/lib/foundry/types";

import { actionFailure, clientIp, requireCookie } from "../lib/foundry-action";

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

const REPORT_CARD_LIMIT = 5;

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
    checksUnevaluable: report.checksUnevaluable,
    missingTools: report.missingTools,
    stubbedTools: report.stubbedTools,
    networkWrites: report.networkWrites,
    shots: report.shots.length,
    // Label only — the raw absolute evidence path never leaves the server.
    evidence: report.evidencePath ? evidenceLabel(report.evidencePath) : null,
    evidenceHref: report.evidencePath
      ? `/foundry/console/evidence/${report.id}`
      : null,
    replayHref: report.trajectoryId
      ? `/foundry/console/trajectories/${report.trajectoryId}`
      : null,
    gameHref: null,
  };
}

// Behind nginx the node server sees its loopback origin; the forwarded
// headers (set in config/nginx/conf.d/_proxy.inc) carry the address the
// visitor can actually open. FOUNDRY_PUBLIC_ORIGIN pins it outright for a
// deployment whose forwarded headers cannot be trusted end to end.
function publicOrigin(request: Request): string {
  const pinned = process.env.FOUNDRY_PUBLIC_ORIGIN;
  if (pinned) return pinned.replace(/\/+$/, "");
  const url = new URL(request.url);
  const proto = request.headers.get("x-forwarded-proto") ?? url.protocol.replace(":", "");
  const host = request.headers.get("x-forwarded-host") ?? url.host;
  return `${proto}://${host}`;
}

export async function loader({ params, request }: Route.LoaderArgs) {
  const { sid, wrap } = sidLoader(request);
  const slug = params.slug;

  try {
    const db = getPool();
    const scene = await getScene(db, slug);
    if (!scene) throw data(null, { status: 404 });

    const [
      embed,
      allReports,
      gddChain,
      readings,
      changelogRows,
      trajectories,
      runsTotal,
      benchAll,
      occurrences,
      memory,
      stewards,
      transfers,
      summary,
      jobs,
    ] = await Promise.all([
      sceneEmbed(scene),
      listBenchReports(db, scene.id, 200),
      scene.gddDocId ? getGddChain(db, scene.gddDocId) : [],
      // The adjacency reading matters only when no doc is truly linked — a
      // linked game's design nodes are its chain, not a neighbor's reading.
      scene.gddDocId ? [] : readingForScene(db, scene.id),
      listSceneChangelog(db, scene.id),
      listTrajectories(db, { sceneId: scene.id, limit: 200 }),
      countTrajectories(db, { sceneId: scene.id }),
      benchSummaryByScene(db),
      listUpcoming(sid, scene.id),
      listSceneMemory(scene.id),
      listStewards(scene.id, sid),
      listTransfers(scene.id),
      continuitySummary(db, scene.id),
      listSceneJobs(db, scene.id),
    ]);

    // A reading names the exact doc version it was read against, and
    // supersede-on-edit mints new ids (gdd-edit.server.ts) — the spine node
    // links each reading's chain head, carrying the read-against version.
    const conceptReadings: FdGameConceptReadingVM[] = await Promise.all(
      readings.map(async (r) => {
        const head = await getGddChainHead(db, r.gddDocId);
        const moved = head !== null && head.id !== r.gddDocId;
        return {
          docId: moved ? head.id : r.gddDocId,
          docTitle: moved ? head.title : r.docTitle,
          readAt: r.readAt,
          rationale: r.rationale,
          confidence: r.confidence,
          readAgainstVersion: moved ? r.docVersion : null,
        };
      }),
    );

    const cardReports = allReports.slice(0, REPORT_CARD_LIMIT);
    const reports: FdBenchReportVM[] = await Promise.all(
      cardReports.map(async (r) => ({
        ...toReportVM(r),
        shotHrefs: await reportShotHrefs(r),
      })),
    );

    const reportByTraj = new Map(
      allReports
        .filter((r) => r.trajectoryId !== null)
        .map((r) => [r.trajectoryId as string, r]),
    );
    const runs: FdGameRunVM[] = trajectories.map((t) => ({
      id: t.id,
      provenance: t.provenance,
      runner: t.runner,
      createdAt: t.createdAt,
      events: t.events,
      finishKind: t.finishReason?.kind ?? null,
      verdict:
        t.runner === "arena" ? null : (reportByTraj.get(t.id)?.verdict ?? null),
      checksFailed: reportByTraj.get(t.id)?.checksFailed ?? null,
      checksTotal: reportByTraj.get(t.id)?.checksTotal ?? null,
      checksUnevaluable: reportByTraj.get(t.id)?.checksUnevaluable ?? null,
    }));
    const benchOnlyRuns: FdGameBenchOnlyRunVM[] = allReports
      .filter((r) => r.trajectoryId === null)
      .map((r) => ({
        id: r.id,
        ranAt: r.ranAt,
        checksFailed: r.checksFailed,
        checksTotal: r.checksTotal,
        checksUnevaluable: r.checksUnevaluable,
      }));

    const summaryRow = benchAll.find((b) => b.sceneId === scene.id) ?? null;
    // The named checks behind the header's "N of M" — from the same run the
    // verdict was read from (the latest real run that recorded a trajectory).
    const latestReal = allReports.find(
      (r) => r.runner !== "arena" && r.trajectoryId !== null,
    );
    const [checklist, demandAsks] = await Promise.all([
      latestReal?.trajectoryId
        ? benchChecklist(db, latestReal.trajectoryId)
        : Promise.resolve([]),
      listShelfAnswerAsks(scene.id, db),
    ]);
    const benchSummary: FdGameBenchSummaryVM | null = summaryRow
      ? {
          runs: summaryRow.runs,
          sandboxRuns: summaryRow.sandboxRuns,
          lastVerdict: summaryRow.lastVerdict,
          lastChecksFailed: summaryRow.lastChecksFailed,
          lastChecksTotal: summaryRow.lastChecksTotal,
          lastChecksUnevaluable: summaryRow.lastChecksUnevaluable,
          lastRealRanAt: summaryRow.lastRealRanAt,
        }
      : null;

    const gatherings: FdGameGatheringVM[] = occurrences.map((o) => ({
      seriesId: o.seriesId,
      title: o.title,
      occurrenceAt: o.occurrenceAt,
      rsvpCount: o.rsvpCount,
    }));

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
        sourceNote: scene.sourceNote,
        description: scene.description,
        thumbnailUrl: scene.thumbnailUrl,
        entityId: scene.entityId,
        entityHref: entityHref(scene.entityId),
        // Labels only — the raw absolute repo/manifest paths never leave the server.
        repoLabel: scene.repoPath ? evidenceLabel(scene.repoPath) : null,
        botManifestLabel: scene.botManifest ? evidenceLabel(scene.botManifest) : null,
      },
      embed,
      worldHref: worldLink(scene),
      editorHref: editorUrl(scene),
      gddHref: scene.gddDocId ? `/foundry/gdd/${scene.gddDocId}` : null,
      // This program's own market-cell reading of the game; null = not yet read.
      marketCell: scene.marketCell,
      // This program's emotional-job reading; empty = not yet read.
      emotionalJobs: jobs,
      designVersions: gddChain.map((c) => ({
        id: c.id,
        version: c.version,
        createdAt: c.createdAt,
        markers:
          c.honesty.open + c.honesty.tbd + c.honesty.hypothesis + c.honesty.agentDecided,
      })) satisfies FdGameDesignVersionVM[],
      conceptReadings,
      steward: stewards.active[0] ?? null,
      checklist:
        latestReal && checklist.length > 0
          ? { ranAt: latestReal.ranAt, rows: checklist }
          : null,
      demandAsks,
      changelog: changelogRows satisfies FdGameChangelogVM[],
      runs,
      runsTotal,
      benchOnlyRuns,
      benchSummary,
      todayIso: new Date().toISOString(),
      reports,
      gatherings,
      continuity: {
        summary: {
          changelog: summary.changelog,
          reports: summary.reports,
          reportsAll: summary.reportsAll,
          episodes: summary.episodes,
          docs: summary.docs,
          stewards: summary.stewards,
        } satisfies FdContinuitySummaryVM,
        memory: memory satisfies FdSceneMemoryRowVM[],
        stewards: {
          active: stewards.active satisfies FdStewardVM[],
          past: stewards.past satisfies FdStewardVM[],
        },
        transfers: transfers satisfies FdTransferVM[],
        isSteward: stewards.isViewerSteward,
        exportHref: `/foundry/continuity/${encodeURIComponent(scene.id)}/export`,
      },
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
      marketCell: null,
      emotionalJobs: null,
      designVersions: [] as FdGameDesignVersionVM[],
      conceptReadings: [] as FdGameConceptReadingVM[],
      changelog: [] as FdGameChangelogVM[],
      runs: [] as FdGameRunVM[],
      runsTotal: 0,
      benchOnlyRuns: [] as FdGameBenchOnlyRunVM[],
      benchSummary: null as FdGameBenchSummaryVM | null,
      todayIso: new Date().toISOString(),
      reports: [] as FdBenchReportVM[],
      gatherings: [] as FdGameGatheringVM[],
      continuity: null,
    });
  }
}

export async function action({ params, request }: Route.ActionArgs) {
  const base = sidLoader(request);
  const sid = base.sid;
  const ip = clientIp(request);

  let form: FormData;
  try {
    form = await request.formData();
  } catch {
    return data(
      { ok: false, intent: "", transferUrl: null, error: "Could not read the form." },
      { status: 400 },
    );
  }
  const intent = String(form.get("intent") ?? "");

  try {
    requireCookie(base.created);
    const scene = await getScene(getPool(), params.slug);
    if (!scene) {
      return data(
        { ok: false, intent, transferUrl: null, error: "That game does not exist." },
        { status: 404 },
      );
    }
    const ctx = { sid: sidBadge(sid) };
    switch (intent) {
      case "claim": {
        const basis = String(form.get("basis") ?? "");
        await claimSteward({ sceneId: scene.id, sid, basis, ip });
        track("fd_steward_claimed", { slug: params.slug }, ctx);
        return { ok: true, intent, transferUrl: null, error: null };
      }
      case "release": {
        await releaseSteward({ sceneId: scene.id, sid, ip });
        track("fd_steward_released", { slug: params.slug }, ctx);
        return { ok: true, intent, transferUrl: null, error: null };
      }
      case "note": {
        const note = String(form.get("note") ?? "");
        await addSceneNote({ sceneId: scene.id, sid, note, ip });
        track("fd_scene_note_added", { slug: params.slug }, ctx);
        return { ok: true, intent, transferUrl: null, error: null };
      }
      case "offer": {
        const note = String(form.get("note") ?? "");
        const { code } = await offerTransfer({ sceneId: scene.id, sid, note, ip });
        track("fd_transfer_offered", { slug: params.slug }, ctx);
        // The one-time link travels only in this action's fetcher data — never
        // stored, never logged, never in loader data.
        const transferUrl = new URL(
          `/foundry/succession/${code}`,
          publicOrigin(request),
        ).toString();
        return { ok: true, intent, transferUrl, error: null };
      }
      case "revoke": {
        const transferId = String(form.get("transferId") ?? "");
        await revokeTransfer({ transferId, sid, ip });
        track("fd_transfer_revoked", { slug: params.slug }, ctx);
        return { ok: true, intent, transferUrl: null, error: null };
      }
      default:
        return { ok: false, intent, transferUrl: null, error: "Unknown action." };
    }
  } catch (err) {
    return actionFailure("foundry.play", intent, err, { transferUrl: null });
  }
}

export default function FoundryGame({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  const ctx = { sid: d.badge };
  const [embedStarted, setEmbedStarted] = useState(false);
  const fetcher = useFetcher<typeof action>();
  const pending = fetcher.state !== "idle";
  const submit = (fields: Record<string, string>) =>
    fetcher.submit(fields, { method: "post" });

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current || !d.game) return;
    viewed.current = true;
    track("fd_game_viewed", { slug: d.slug }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.badge, d.slug]);

  if (d.unavailable || !d.game) {
    return (
      <div className="fd-page fd-stack">
        <FdPageHead
          eyebrow="Games"
          title="Game"
          crumbs={<a href="/foundry/play">← All games</a>}
        />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return (
    <FdGameDetail
      game={d.game}
      steward={d.steward}
      checklist={d.checklist}
      demandAsks={d.demandAsks}
      embed={d.embed}
      worldHref={d.worldHref}
      editorHref={d.editorHref}
      gddHref={d.gddHref}
      marketCell={d.marketCell}
      emotionalJobs={d.emotionalJobs}
      designVersions={d.designVersions}
      conceptReadings={d.conceptReadings}
      changelog={d.changelog}
      runs={d.runs}
      runsTotal={d.runsTotal}
      benchOnlyRuns={d.benchOnlyRuns}
      benchSummary={d.benchSummary}
      todayIso={d.todayIso}
      reports={d.reports}
      gatherings={d.gatherings}
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
      onHistoryNodeOpen={(kind) =>
        track("fd_history_node_opened", { slug: d.slug, kind }, ctx)
      }
      responseHref={`/foundry/play/${encodeURIComponent(d.slug)}/response`}
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
      continuity={
        d.continuity ? (
          <FdContinuity
            slug={d.slug}
            summary={d.continuity.summary}
            memory={d.continuity.memory}
            stewards={d.continuity.stewards}
            transfers={d.continuity.transfers}
            isSteward={d.continuity.isSteward}
            exportHref={d.continuity.exportHref}
            mintedTransferUrl={fetcher.data?.transferUrl ?? null}
            error={fetcher.data && !fetcher.data.ok ? fetcher.data.error : null}
            pending={pending}
            onClaim={(basis) => submit({ intent: "claim", basis })}
            onRelease={() => submit({ intent: "release" })}
            onNote={(note) => submit({ intent: "note", note })}
            onOfferTransfer={(note) => submit({ intent: "offer", note })}
            onRevokeTransfer={(transferId) => submit({ intent: "revoke", transferId })}
          />
        ) : null
      }
    />
  );
}
