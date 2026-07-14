import { evidenceLabel } from "@ui/foundry/components/evidence";

import { FoundryUnavailableError, getPool } from "@data/lib/foundry/db.server";
import { buildProjectBundle } from "@data/lib/foundry/export.server";
import { getScene } from "@data/lib/foundry/scenes.server";

import type { Route } from "./+types/foundry.continuity_.$sceneId.export";

// Resource route: the downloadable project bundle. The edge sanitation the
// export module leaves to its caller happens here — host filesystem paths
// reduced to stable labels or repo-relative paths so the standing record does
// not rot with the server's scratch directories. Steward rows arrive already
// sid-free: listStewards strips the raw sid inside the data layer.

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body, null, 2), {
    status,
    headers: { "content-type": "application/json; charset=utf-8" },
  });
}

// "~/one/rig/play/x" → "one/rig/play/x"; an absolute path is reduced to its
// evidence label rather than shipped whole.
function repoRelative(path: string | null): string | null {
  if (path === null) return null;
  if (path.startsWith("~/")) return path.slice(2);
  return path.startsWith("/") ? evidenceLabel(path) : path;
}

export async function loader({ request, params }: Route.LoaderArgs) {
  const sceneId = params.sceneId;
  try {
    const db = getPool();
    const scene = await getScene(db, sceneId);
    if (!scene) {
      return json(404, { error: "No stored scene carries that id." });
    }
    const bundle = await buildProjectBundle(db, sceneId);
    const sanitized = {
      ...bundle,
      scene: bundle.scene
        ? {
            ...bundle.scene,
            repoPath: repoRelative(bundle.scene.repoPath),
            botManifest: repoRelative(bundle.scene.botManifest),
          }
        : bundle.scene,
      reports: bundle.reports.map((r) => ({
        ...r,
        evidencePath: r.evidencePath ? evidenceLabel(r.evidencePath) : null,
      })),
      trajectories: bundle.trajectories.map((t) => ({
        ...t,
        trajectory: {
          ...t.trajectory,
          evidencePath: t.trajectory.evidencePath
            ? evidenceLabel(t.trajectory.evidencePath)
            : null,
        },
      })),
    };
    const filename = `foundry-${sceneId.replace(/[^A-Za-z0-9._-]/g, "_")}-bundle.json`;
    // ?view serves the same bytes without the attachment disposition, so the
    // bundle is readable in the browser before anyone saves it. The bare URL
    // stays a download, keeping every link already handed out working.
    const inline = new URL(request.url).searchParams.has("view");
    // The download is tracked CLIENT-side, on the anchor's click: a loader
    // track lets any bare GET — a crawler, a sweep agent — mint download
    // evidence without a person or even a browser (observed: 37 such rows).
    return new Response(JSON.stringify(sanitized, null, 2), {
      headers: {
        "content-type": "application/json; charset=utf-8",
        ...(inline
          ? {}
          : { "content-disposition": `attachment; filename="${filename}"` }),
        "cache-control": "no-store",
      },
    });
  } catch (err) {
    if (!(err instanceof FoundryUnavailableError)) throw err;
    return json(503, { error: "The Foundry database is not configured." });
  }
}
