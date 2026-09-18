
import { z } from "zod";

import { catalystBase } from "../client";
import { warnInvalid } from "../warn";
import { ReportRowSchema, type ReportRow } from "./places-moderation";
import {
  available,
  unavailable,
  unavailableFromStatus,
  type ControlResult,
} from "./availability";

const SERVER_CHECK_GATE =
  "catalyrst-places/src/handlers/admin.rs:13-15 -> catalyrst-places/src/auth.rs:88-100";

const NOT_CONFIGURED =
  "Places moderation is not configured on this node (PLACES_ADMIN_AUTH_TOKEN unset).";

function assertServerOnly(): void {
  if (typeof window !== "undefined") {
    throw new Error(
      "places-moderation.server.ts was imported into a browser bundle. " +
        "It holds an admin bearer token and must stay server-only.",
    );
  }
}

function adminToken(): string | undefined {
  const t =
    typeof process !== "undefined"
      ? process.env?.PLACES_ADMIN_AUTH_TOKEN
      : undefined;
  return t && t.length > 0 ? t : undefined;
}

const ErrorMessageSchema = z.object({ message: z.string() });

function serverMessage(payload: unknown, fallback: string): string {
  const parsed = ErrorMessageSchema.safeParse(payload);
  return parsed.success && parsed.data.message.trim()
    ? parsed.data.message.trim()
    : fallback;
}

type AdminRequest = {
  method: "GET" | "PATCH";
  path: string;
  query?: Record<string, string | number | undefined>;
  body?: unknown;
  signal?: AbortSignal;
};

type RawResponse =
  | { ok: true; payload: unknown }
  | { ok: false; status: number; message: string };

async function adminFetch(req: AdminRequest): Promise<RawResponse> {
  const token = adminToken();
  if (!token) {
    return { ok: false, status: 503, message: NOT_CONFIGURED };
  }

  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(req.query ?? {})) {
    if (v === undefined || v === "") continue;
    params.set(k, String(v));
  }
  const qs = params.toString();
  const url = `${catalystBase()}${req.path}${qs ? `?${qs}` : ""}`;

  const headers: Record<string, string> = {
    accept: "application/json",
    authorization: `Bearer ${token}`,
  };
  if (req.body !== undefined) headers["content-type"] = "application/json";

  let res: Response;
  try {
    res = await fetch(url, {
      method: req.method,
      headers,
      body: req.body === undefined ? undefined : JSON.stringify(req.body),
      signal: req.signal,
      cache: "no-store",
    });
  } catch (err) {
    return {
      ok: false,
      status: 502,
      message: `Places backend unreachable: ${
        (err as Error)?.message ?? "network error"
      }`,
    };
  }

  let payload: unknown = null;
  try {
    payload = await res.json();
  } catch {
    payload = null;
  }

  if (!res.ok) {
    return {
      ok: false,
      status: res.status,
      message: serverMessage(
        payload,
        `Places backend returned HTTP ${res.status}.`,
      ),
    };
  }
  return { ok: true, payload };
}

const ReportListSchema = z.object({
  data: z.array(z.unknown()),
  total: z.number().nullish().transform((v) => v ?? null),
});

type ReportQueue = {
  rows: ReportRow[];
  total: number;
};

type ReportQueueQuery = {
  status?: string;
  entityId?: string;
  limit?: number;
  offset?: number;
  signal?: AbortSignal;
};

function liftReason(row: ReportRow): ReportRow {
  if (row.reason) return row;
  const payload = row.payload as Record<string, unknown> | null | undefined;
  const reason =
    payload && typeof payload.reason === "string" ? payload.reason : null;
  return reason ? { ...row, reason } : row;
}

export async function loadReportQueue(
  q: ReportQueueQuery = {},
): Promise<ControlResult<ReportQueue>> {
  assertServerOnly();

  const res = await adminFetch({
    method: "GET",
    path: "/places/api/reports",
    query: {
      status: q.status ?? "open",
      entity_id: q.entityId,
      limit: q.limit ?? 50,
      offset: q.offset ?? 0,
    },
    signal: q.signal,
  });

  if (!res.ok) {
    return unavailableFromStatus(res.status, res.message, SERVER_CHECK_GATE);
  }

  const parsed = ReportListSchema.safeParse(res.payload);
  if (!parsed.success) {
    return unavailable(
      "backend-error",
      "Places backend returned an unexpected report-queue shape.",
      { status: 502, serverCheck: SERVER_CHECK_GATE },
    );
  }

  const rows: ReportRow[] = [];
  for (const raw of parsed.data.data) {
    const r = ReportRowSchema.safeParse(raw);
    if (r.success) rows.push(liftReason(r.data));
    else warnInvalid("ReportRow", r.error.issues);
  }
  return available({ rows, total: parsed.data.total ?? rows.length });
}

z.object({
  ok: z.boolean().nullish(),
  data: ReportRowSchema,
});

z.object({
  data: z.object({ id: z.string(), disabled: z.boolean() }),
});

z.object({
  reportId: z.string().min(1),
  entityId: z.string().min(1).nullish(),
  decision: z.enum(["resolve", "dismiss", "action", "reopen"]),
  resolution: z.string().optional(),
  notes: z.string().optional(),
  resolvedBy: z.string().optional(),
  disablePlace: z.boolean().optional(),
  disableReason: z.string().optional(),
});

z.object({
  placeId: z.string().min(1),
  disabled: z.boolean(),
  reason: z.string().optional(),
});

