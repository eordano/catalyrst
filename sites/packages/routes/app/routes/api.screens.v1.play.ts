import { loadPlayScreen, playAddress } from "@data/lib/screens/play.server";

export async function loader({ request }: { request: Request }) {
  if (request.headers.get("accept")?.includes("application/x-ndjson")) {
    const address = playAddress(request);
    const abort = new AbortController();
    const scoped = new Request(request, { signal: AbortSignal.any([request.signal, abort.signal]) });
    const encoder = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        const send = (value: unknown) => controller.enqueue(encoder.encode(JSON.stringify(value) + "\n"));
        void loadPlayScreen(scoped, send).then(({ serverTiming }) => {
          if (abort.signal.aborted) return;
          send({ version: 1, address, done: true, serverTiming });
          controller.close();
        }).catch((error) => {
          if (!abort.signal.aborted) controller.error(error);
        });
      },
      cancel(reason) { abort.abort(reason); },
    });
    return new Response(body, { headers: {
      "Content-Type": "application/x-ndjson; charset=utf-8",
      "Cache-Control": "private, no-store",
      "X-Accel-Buffering": "no",
    } });
  }
  const { data, serverTiming } = await loadPlayScreen(request);
  return Response.json(data, {
    headers: { "Cache-Control": "private, no-store", "Server-Timing": serverTiming },
  });
}
