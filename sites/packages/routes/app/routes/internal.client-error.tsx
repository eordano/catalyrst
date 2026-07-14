import { ensureSid } from "@core/lib/experiments/assign";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/internal.client-error";

const MAX_BODY_BYTES = 16 * 1024;
const MAX_STACK = 8000;
const MAX_LINE = 2000;

const STORY = "sites-error";

export type ClientErrorReport = {
  message: string;
  name: string;
  stack: string;
  url: string;
  ua: string;
  ts: string;
};

function clip(v: unknown, max: number): string {
  if (typeof v !== "string") return "";
  return v.length > max ? v.slice(0, max) : v;
}

export function normalizeReport(raw: unknown): ClientErrorReport | null {
  if (!raw || typeof raw !== "object") return null;
  const r = raw as Record<string, unknown>;
  const message = clip(r.message, MAX_LINE).replace(/\s+/g, " ").trim();
  const name = clip(r.name, 120).trim();
  const stack = clip(r.stack, MAX_STACK);
  if (!message && !stack.trim()) return null;
  return {
    message: message || "(no message)",
    name: name || "Error",
    stack,
    url: clip(r.url, MAX_LINE),
    ua: clip(r.ua, MAX_LINE),
    ts: clip(r.ts, 40) || new Date().toISOString(),
  };
}

export async function action({ request }: Route.ActionArgs) {
  if (request.method !== "POST") {
    return Response.json({ ok: false, error: "method not allowed" }, { status: 405 });
  }

  let raw: string;
  try {
    raw = await request.text();
  } catch {
    return new Response(null, { status: 204 });
  }

  if (raw.length > MAX_BODY_BYTES) {
    return Response.json({ ok: false, error: "payload too large" }, { status: 413 });
  }

  let parsed: unknown;
  try {
    parsed = raw ? JSON.parse(raw) : null;
  } catch {
    return Response.json({ ok: false, error: "invalid json" }, { status: 400 });
  }

  const report = normalizeReport(parsed);
  if (!report) {
    return new Response(null, { status: 204 });
  }

  recordClientError(report, request);
  return Response.json({ ok: true }, { status: 202 });
}

export function recordClientError(report: ClientErrorReport, request: Request): void {
  try {
    // eslint-disable-next-line no-console
    console.error(
      "[client-error]",
      JSON.stringify({
        name: report.name,
        message: report.message,
        url: report.url,
        ua: report.ua,
        ts: report.ts,
        stack: report.stack.split("\n").slice(0, 12).join("\n"),
      }),
    );
  } catch {
  }

  try {
    const { sid } = ensureSid(request);
    track(
      "client_error",
      {
        error_name: report.name,
        error_message: report.message,
        url: report.url,
        ua: report.ua,
        client_ts: report.ts,
      },
      { sid, story: STORY },
    );
  } catch {
  }
}
