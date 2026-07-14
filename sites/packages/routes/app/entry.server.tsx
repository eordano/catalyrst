import { PassThrough } from "node:stream";

import type { EntryContext } from "react-router";
import { createReadableStreamFromReadable } from "@react-router/node";
import { ServerRouter } from "react-router";
import { isbot } from "isbot";
import type { RenderToPipeableStreamOptions } from "react-dom/server";
import { renderToPipeableStream } from "react-dom/server";

import { markdownResponse, wantsMarkdown } from "@data/lib/agent/markdown";
import { renderAgentMarkdown } from "@data/lib/agent/dispatch.server";
import { ensureSid } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";

export const streamTimeout = 5_000;

export default function handleRequest(
  request: Request,
  responseStatusCode: number,
  responseHeaders: Headers,
  routerContext: EntryContext,
  _loadContext: unknown,
) {
  if (wantsMarkdown(request)) {
    const hit = renderAgentMarkdown(routerContext);
    if (hit) return markdownResponse(hit.md, { status: hit.status });
  }

  if (responseStatusCode === 404) {
    try {
      const { sid } = ensureSid(request);
      const u = new URL(request.url);
      track(
        "page_not_found",
        {
          path: u.pathname + u.search,
          referrer: request.headers.get("referer") ?? "",
          ua: request.headers.get("user-agent") ?? "",
        },
        { sid, story: "sites-error" },
      );
    } catch {
    }
  }

  return new Promise((resolve, reject) => {
    let shellRendered = false;
    const userAgent = request.headers.get("user-agent");

    const readyOption: keyof RenderToPipeableStreamOptions =
      (userAgent && isbot(userAgent)) || routerContext.isSpaMode
        ? "onAllReady"
        : "onShellReady";

    const { pipe, abort } = renderToPipeableStream(
      <ServerRouter context={routerContext} url={request.url} />,
      {
        [readyOption]() {
          shellRendered = true;
          const body = new PassThrough();
          const stream = createReadableStreamFromReadable(body);

          responseHeaders.set("Content-Type", "text/html");

          resolve(
            new Response(stream, {
              headers: responseHeaders,
              status: responseStatusCode,
            }),
          );

          pipe(body);
        },
        onShellError(error: unknown) {
          reject(error);
        },
        onError(error: unknown) {
          responseStatusCode = 500;
          if (shellRendered) {
            console.error(error);
          }
        },
      },
    );

    setTimeout(abort, streamTimeout + 1_000);
  });
}
