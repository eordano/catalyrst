import FdContinuityPage, {
  type FdContinuityDetailVM,
  type FdContinuitySceneVM,
} from "@ui/foundry/pages/FdContinuityPage";

import { FD_UNAVAILABLE, FdPageHead } from "@ui/foundry/components/FdSection";

import "@ui/foundry/components/fdsection.css";
import "@ui/foundry/components/fdstat.css";
import "@ui/foundry/components/fdtable.css";
import "@ui/foundry/components/fdpersonachip.css";
import "@ui/foundry/pages/fdcontinuity.css";

import { sidLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import {
  listSceneMemory,
  listStewards,
  listTransfers,
} from "@data/lib/foundry/continuity.server";
import {
  FoundryUnavailableError,
  getPool,
  sidBadge,
} from "@data/lib/foundry/db.server";
import { continuitySummary } from "@data/lib/foundry/export.server";
import { listScenes } from "@data/lib/foundry/scenes.server";

import type { Route } from "./+types/foundry.continuity";

export function meta() {
  return [{ title: "Continuity — The Foundry" }];
}

export async function loader({ request }: Route.LoaderArgs) {
  const base = sidLoader(request);
  const url = new URL(request.url);
  const selected = url.searchParams.get("scene");

  try {
    const db = getPool();
    const scenes = await listScenes(db);
    const sceneVMs: FdContinuitySceneVM[] = await Promise.all(
      scenes.map(async (s) => ({
        id: s.id,
        title: s.title,
        worldName: s.worldName,
        deployedAt: s.deployedAt,
        importedAt: s.importedAt,
        sizeBytes: s.sizeBytes,
        parcels: s.parcels,
        source: s.source,
        sourceNote: s.sourceNote,
        counts: await continuitySummary(db, s.id),
        exportHref: `/foundry/continuity/${encodeURIComponent(s.id)}/export`,
      })),
    );

    let detail: FdContinuityDetailVM | null = null;
    let selectedMissing = false;
    if (selected) {
      const scene = sceneVMs.find((v) => v.id === selected) ?? null;
      if (!scene) {
        selectedMissing = true;
      } else {
        const [memory, stewards, transfers] = await Promise.all([
          listSceneMemory(selected),
          listStewards(selected),
          listTransfers(selected),
        ]);
        detail = {
          scene,
          memory,
          stewards: {
            active: stewards.active,
            past: stewards.past,
          },
          transfers: transfers.map((t) => ({
            id: t.id,
            from: t.from,
            note: t.note,
            effectiveStatus: t.effectiveStatus,
            createdAt: t.createdAt,
            expiresAt: t.expiresAt,
            acceptedAt: t.acceptedAt,
            acceptedBy: t.acceptedBy,
          })),
        };
      }
    }

    track("fd_continuity_viewed", {}, { sid: sidBadge(base.sid) });
    return base.wrap({
      badge: sidBadge(base.sid),
      unavailable: false,
      scenes: sceneVMs,
      selected,
      selectedMissing,
      detail,
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    return base.wrap({
      badge: sidBadge(base.sid),
      unavailable: true,
      scenes: [] as FdContinuitySceneVM[],
      selected,
      selectedMissing: false,
      detail: null as FdContinuityDetailVM | null,
    });
  }
}

export default function FoundryContinuity({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  if (d.unavailable) {
    return (
      <div className="fd-page fd-stack fd-continuity">
        <FdPageHead title="The standing record" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }
  return (
    <FdContinuityPage
      scenes={d.scenes}
      selected={d.selected}
      selectedMissing={d.selectedMissing}
      detail={d.detail}
      onDownload={(sceneId) =>
        track("fd_bundle_downloaded", { scene_id: sceneId }, { sid: d.badge })
      }
    />
  );
}
