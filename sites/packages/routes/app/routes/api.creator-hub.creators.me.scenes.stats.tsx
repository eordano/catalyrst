import { verifyAuthChainRequest } from "@data/lib/catalyst/creator-hub/scene-drafts.server";
import { loadCreatorSceneStats } from "@data/lib/catalyst/creator-hub/scene-analytics.server";

export async function loader({ request }: { request: Request }) {
  const headers = { "Cache-Control": "private, no-store" };
  const auth = await verifyAuthChainRequest(request);
  if (!auth.ok) return Response.json({ error: auth.error }, { status: auth.status, headers });
  try {
    const signal = AbortSignal.any([request.signal, AbortSignal.timeout(20000)]);
    return Response.json(await loadCreatorSceneStats(auth.wallet, { signal }), { headers });
  } catch {
    return Response.json({ error: "Scene activity is temporarily unavailable. Try again." }, { status: 503, headers });
  }
}
