/**
 * Server-only data layer for the places report queue.
 *
 * Server-side authorization, read in full before this module was written:
 *
 *   catalyrst/crates/catalyrst-places/src/handlers/admin.rs:13-15  `gate()`
 *     -> catalyrst-places/src/auth.rs:88-100  `require_admin_bearer`
 *        :90-91  `expected: None`      -> 403 "Admin token not configured"
 *        :92-96  bearer mismatch/absent -> 403 "Invalid admin credentials"
 *        the compare at :80-86 is timing-safe.
 *
 *   `gate()` is the first statement of all three handlers:
 *     get_reports          admin.rs:36-41
 *     patch_report         admin.rs:77-83
 *     patch_place_disable  admin.rs:125-131
 *
 * The credential is `PLACES_ADMIN_AUTH_TOKEN`
 * (catalyrst-places/src/config.rs:49). It is a server-to-server bearer and
 * must never reach a browser bundle -- that is the entire reason this file is
 * `.server.ts` and the reason the browser-side write in `places-moderation.ts`
 * was removed.
 *
 * On this node the token is unset: it appears in no `deploy/env/*.env` file
 * and not in `deploy/env/sites.env`. Every export below therefore answers
 * `not-configured` today. That is the correct, fail-closed outcome --
 * provisioning the token is a separate, deliberate act and is not part of this
 * change.
 */

import { z } from "zod";

import { catalystBase } from "../client";
import { warnInvalid } from "../warn";
import {
  ReportRowSchema,
  decisionToStatus,
  type DisablePlaceBody,
  type ModerationDecision,
  type ReportPatchBody,
  type ReportRow,
} from "./places-moderation";
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

/*
 * Row 1 -- read the report queue.
 * GET {catalyst}/places/api/reports
 */

/**
 * `data` is required so a report queue that did not arrive cannot arrive as an
 * empty one. A failed parse takes the `unavailable("backend-error")` path
 * below, which the page renders as a reason; `data: []` would have rendered as
 * "no open reports".
 */
const ReportListSchema = z.object({
  data: z.array(z.unknown()),
  total: z.number().nullish().transform((v) => v ?? null),
});

export type ReportQueue = {
  rows: ReportRow[];
  total: number;
};

export type ReportQueueQuery = {
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

/*
 * Rows 2 & 3 -- commit a moderation decision, and optionally disable
 * the reported place.
 * PATCH {catalyst}/places/api/reports/{id}
 * PATCH {catalyst}/places/api/places/{id}/disable
 */

const ReportPatchResponseSchema = z.object({
  ok: z.boolean().nullish(),
  data: ReportRowSchema,
});

const DisablePlaceResponseSchema = z.object({
  data: z.object({ id: z.string(), disabled: z.boolean() }),
});

const ModerateInputSchema = z.object({
  reportId: z.string().min(1),
  entityId: z.string().min(1).nullish(),
  decision: z.enum(["resolve", "dismiss", "action", "reopen"]),
  resolution: z.string().optional(),
  notes: z.string().optional(),
  resolvedBy: z.string().optional(),
  disablePlace: z.boolean().optional(),
  disableReason: z.string().optional(),
});

export type ModerationCommit = {
  report: ReportRow;
  placeDisabled: boolean;
  reportBody: ReportPatchBody;
  disableBody?: DisablePlaceBody;
};

export async function commitModerationDecision(
  raw: unknown,
  opts: { signal?: AbortSignal } = {},
): Promise<ControlResult<ModerationCommit>> {
  assertServerOnly();

  const input = ModerateInputSchema.safeParse(raw);
  if (!input.success) {
    return unavailable("backend-error", "Invalid moderation request.", {
      status: 400,
      serverCheck: SERVER_CHECK_GATE,
    });
  }
  const args = input.data;

  const decision = args.decision as ModerationDecision;
  const reopening = decision === "reopen";
  const reportBody: ReportPatchBody = {
    status: decisionToStatus(decision),
    resolution: reopening ? undefined : args.resolution || undefined,
    notes: args.notes || undefined,
    resolved_by: reopening ? undefined : args.resolvedBy || "moderator",
  };

  const patchRes = await adminFetch({
    method: "PATCH",
    path: `/places/api/reports/${encodeURIComponent(args.reportId)}`,
    body: reportBody,
    signal: opts.signal,
  });
  if (!patchRes.ok) {
    return unavailableFromStatus(
      patchRes.status,
      patchRes.message,
      SERVER_CHECK_GATE,
    );
  }

  const patched = ReportPatchResponseSchema.safeParse(patchRes.payload);
  if (!patched.success) {
    return unavailable(
      "backend-error",
      "Places backend returned an unexpected response to the report patch.",
      { status: 502, serverCheck: SERVER_CHECK_GATE },
    );
  }

  const wantsDisable = args.disablePlace === true && Boolean(args.entityId);
  const disableBody: DisablePlaceBody | undefined = wantsDisable
    ? { disabled: true, reason: args.disableReason || undefined }
    : undefined;

  let placeDisabled = false;
  if (disableBody && args.entityId) {
    const disableRes = await adminFetch({
      method: "PATCH",
      path: `/places/api/places/${encodeURIComponent(args.entityId)}/disable`,
      body: disableBody,
      signal: opts.signal,
    });
    if (!disableRes.ok) {
      // The report patch already landed. Say so rather than implying the whole
      // decision failed.
      return unavailableFromStatus(
        disableRes.status,
        `Report was updated, but disabling the place failed: ${disableRes.message}`,
        SERVER_CHECK_GATE,
      );
    }
    const parsed = DisablePlaceResponseSchema.safeParse(disableRes.payload);
    placeDisabled = parsed.success ? parsed.data.data.disabled : true;
  }

  return available({
    report: liftReason(patched.data.data),
    placeDisabled,
    reportBody,
    disableBody,
  });
}

/*
 * Row 3 standalone -- disable / re-enable a place without a report.
 * PATCH {catalyst}/places/api/places/{id}/disable
 */

const DisableInputSchema = z.object({
  placeId: z.string().min(1),
  disabled: z.boolean(),
  reason: z.string().optional(),
});

export async function setPlaceDisabled(
  raw: unknown,
  opts: { signal?: AbortSignal } = {},
): Promise<ControlResult<{ id: string; disabled: boolean }>> {
  assertServerOnly();

  const input = DisableInputSchema.safeParse(raw);
  if (!input.success) {
    return unavailable("backend-error", "Invalid disable request.", {
      status: 400,
      serverCheck: SERVER_CHECK_GATE,
    });
  }
  const { placeId, disabled, reason } = input.data;

  const res = await adminFetch({
    method: "PATCH",
    path: `/places/api/places/${encodeURIComponent(placeId)}/disable`,
    body: { disabled, reason: reason?.trim() || undefined },
    signal: opts.signal,
  });
  if (!res.ok) {
    return unavailableFromStatus(res.status, res.message, SERVER_CHECK_GATE);
  }

  const parsed = DisablePlaceResponseSchema.safeParse(res.payload);
  if (!parsed.success) {
    return unavailable(
      "backend-error",
      "Places backend returned an unexpected response to the disable patch.",
      { status: 502, serverCheck: SERVER_CHECK_GATE },
    );
  }
  return available(parsed.data.data);
}
