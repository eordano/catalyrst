import { z } from "zod";

import { shortAddress } from "../format/address";

export const REPORT_STATUSES = [
  "open",
  "resolved",
  "dismissed",
  "actioned",
] as const;
export type ReportStatus = (typeof REPORT_STATUSES)[number];

export const MODERATION_DECISIONS = [
  "resolve",
  "dismiss",
  "action",
  "reopen",
] as const;
export type ModerationDecision = (typeof MODERATION_DECISIONS)[number];

export function decisionToStatus(decision: ModerationDecision): ReportStatus {
  switch (decision) {
    case "resolve":
      return "resolved";
    case "dismiss":
      return "dismissed";
    case "action":
      return "actioned";
    case "reopen":
      return "open";
  }
}

export type ReportPatchBody = {
  status: ReportStatus;
  resolution?: string;
  notes?: string;
  resolved_by?: string;
};

export type DisablePlaceBody = {
  disabled: boolean;
  reason?: string;
};

const nullableStr = z.string().nullish().transform((v) => v ?? null);

/**
 * `reporter` and `created_at` are required.
 *
 * Both are non-optional on the wire -- `catalyrst-places/src/ports/places/rows.rs:187-199`
 * declares `reporter: String` and `created_at: DateTime<Utc>`. Defaulting them
 * put a report that failed to arrive in the queue anyway, attributed to a
 * literal reporter named "unknown" and stamped 1970-01-01, which sorts to the
 * top of an oldest-first queue and reads as a real, very old report. A row
 * missing either is dropped by `loadReportQueue`.
 */
export const ReportRowSchema = z.object({
  id: z.union([z.number(), z.string()]).transform((v) => String(v)),
  entity_id: nullableStr,
  reporter: z.string(),
  status: z
    .string()
    .nullish()
    .transform((v) => (v ?? "open") as ReportStatus),
  reason: nullableStr,
  resolution: nullableStr,
  notes: nullableStr,
  resolved_by: nullableStr,
  resolved_at: nullableStr,
  created_at: z.string(),
  place_title: nullableStr,
  place_coords: nullableStr,
  place_image: nullableStr,
  place_creator: nullableStr,
  payload: z.unknown().nullish(),
});
export type ReportRow = z.infer<typeof ReportRowSchema>;

export const OptionSchema = z.object({ code: z.string(), label: z.string() });
export type Option = z.infer<typeof OptionSchema>;

export const REPORT_REASONS: Option[] = [
  { code: "violent_or_graphic", label: "Violent or graphic content" },
  { code: "hateful_or_abusive", label: "Hateful or abusive content" },
  { code: "adult_content", label: "Adult or sexual content" },
  { code: "scam_or_spam", label: "Scam, spam or misleading" },
  { code: "intellectual_property", label: "Intellectual property violation" },
  { code: "other", label: "Other (see notes)" },
];

export const RESOLUTION_OPTIONS: Option[] = [
  { code: "no_violation", label: "No violation \u{2014} content within policy" },
  { code: "warning_issued", label: "Warning issued to creator" },
  { code: "content_removed", label: "Content removed / scene disabled" },
  { code: "duplicate", label: "Duplicate report" },
  { code: "insufficient_evidence", label: "Insufficient evidence" },
];

/**
 * Every list is required: this is a moderation queue, and a body that carries
 * no `reports` key is a body we did not understand. An empty array here would
 * have said "nothing to moderate", which is the one answer this page must never
 * invent.
 */
export const ModerationFixtureSchema = z.object({
  reports: z.array(ReportRowSchema),
  reasons: z.array(OptionSchema),
  resolutions: z.array(OptionSchema),
});
export type ModerationFixture = z.infer<typeof ModerationFixtureSchema>;

/**
 * There is no browser-side report-queue read here either.
 *
 * `fetchReportQueue` used to live at this spot: an unauthenticated
 * `GET /places/api/reports` from client code, against an endpoint whose first
 * statement is `gate()`
 * (`catalyrst-places/src/handlers/admin.rs:41` -> `auth.rs:88-100`). It could
 * only ever 403, and its last caller is gone: the route loader now calls
 * `loadReportQueue` from `places-moderation.server.ts`, where the bearer is.
 */

function liftReason(row: ReportRow): ReportRow {
  if (row.reason) return row;
  const payload = row.payload as Record<string, unknown> | null | undefined;
  const reason = payload && typeof payload.reason === "string" ? payload.reason : null;
  return reason ? { ...row, reason } : row;
}

function hueFor(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360;
  return h;
}

export function reportTimeLabel(iso: string | null): string {
  if (!iso) return "\u{2014}";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "\u{2014}";
  return d
    .toLocaleString("en-US", {
      day: "numeric",
      month: "short",
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
      timeZone: "UTC",
    })
    .toUpperCase();
}

export function shortAddr(addr: string): string {
  return addr ? shortAddress(addr) : "unknown";
}

export function reasonLabel(reasons: Option[], code: string | null): string {
  if (!code) return "Unspecified";
  const hit = reasons.find((r) => r.code === code);
  if (hit) return hit.label;
  return code.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

export function statusLabel(status: ReportStatus): string {
  switch (status) {
    case "open":
      return "Open";
    case "resolved":
      return "Resolved";
    case "dismissed":
      return "Dismissed";
    case "actioned":
      return "Actioned";
  }
}

export type ReportCard = {
  id: string;
  entityId: string | null;
  status: ReportStatus;
  reason: string | null;
  reporter: string;
  reporterShort: string;
  createdLabel: string;
  placeTitle: string;
  placeCoords: string | null;
  placeImage: string | null;
  placeCreator: string | null;
  resolution: string | null;
  notes: string | null;
  resolvedBy: string | null;
  hue: number;
};

export function toReportCard(row: ReportRow): ReportCard {
  return {
    id: row.id,
    entityId: row.entity_id,
    status: row.status,
    reason: row.reason,
    reporter: row.reporter,
    reporterShort: shortAddr(row.reporter),
    createdLabel: reportTimeLabel(row.created_at),
    placeTitle: row.place_title ?? row.entity_id ?? "Unknown place",
    placeCoords: row.place_coords,
    placeImage: row.place_image,
    placeCreator: row.place_creator,
    resolution: row.resolution,
    notes: row.notes,
    resolvedBy: row.resolved_by,
    hue: hueFor(row.id),
  };
}

export type QueueBuckets = {
  open: ReportCard[];
  resolved: ReportCard[];
  dismissed: ReportCard[];
  actioned: ReportCard[];
};

export function bucketReports(cards: ReportCard[]): QueueBuckets {
  const out: QueueBuckets = { open: [], resolved: [], dismissed: [], actioned: [] };
  for (const c of cards) out[c.status].push(c);
  return out;
}

export type ModerationResult = {
  report: ReportRow;
  placeDisabled: boolean;
  reportBody: ReportPatchBody;
  disableBody?: DisablePlaceBody;
};

/**
 * Shape the `/admin/places-decision` action answers with on success.
 * `commitModerationDecision` sets `placeDisabled` on every success path
 * (places-moderation.server.ts:337), so a body without it is not a decision
 * that left the place up -- it is not this response, and defaulting it to
 * `false` reported a place still live when it may have just been taken down.
 */
export const ModerationResultSchema = z.object({
  report: ReportRowSchema,
  placeDisabled: z.boolean(),
  reportBody: z.object({
    status: z.enum(REPORT_STATUSES),
    resolution: z.string().optional(),
    notes: z.string().optional(),
    resolved_by: z.string().optional(),
  }),
  disableBody: z
    .object({ disabled: z.boolean(), reason: z.string().optional() })
    .optional(),
});

export async function simulateModerateReport(
  args: {
    report: ReportRow;
    decision: ModerationDecision;
    resolution?: string;
    notes?: string;
    resolvedBy?: string;
    disablePlace?: boolean;
    disableReason?: string;
  },
  opts: { signal?: AbortSignal; delayMs?: number } = {},
): Promise<ModerationResult> {
  const { report, decision } = args;
  if (!report?.id) throw new Error("report id is required");

  const status = decisionToStatus(decision);
  const reopening = decision === "reopen";

  const reportBody: ReportPatchBody = {
    status,
    resolution: reopening ? undefined : args.resolution || undefined,
    notes: args.notes || undefined,
    resolved_by: reopening ? undefined : args.resolvedBy || "moderator",
  };

  const disablePlace = args.disablePlace ?? false;
  const disableBody: DisablePlaceBody | undefined =
    disablePlace && report.entity_id
      ? { disabled: true, reason: args.disableReason || undefined }
      : undefined;
  void reportBody;
  void disableBody;

  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, opts.delayMs ?? 500);
    opts.signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });

  const nowIso = new Date().toISOString();
  const patched: ReportRow = {
    ...report,
    status,
    resolution: reopening ? null : args.resolution ?? report.resolution ?? null,
    notes: args.notes ?? report.notes ?? null,
    resolved_by: reopening ? null : reportBody.resolved_by ?? null,
    resolved_at: reopening ? null : nowIso,
  };

  return {
    report: patched,
    placeDisabled: Boolean(disableBody),
    reportBody,
    disableBody,
  };
}

/**
 * The browser-side privileged write that used to live here has been removed.
 *
 * What it did: read `process.env.PLACES_ADMIN_AUTH_TOKEN` from code that is
 * bundled into the browser (where `process.env` is always empty), found no
 * token, **omitted the `authorization` header, and sent the PATCH anyway**. It
 * did not fail closed locally -- the only thing that stopped it was a 403 from
 * `catalyrst-places/src/auth.rs:88-100`, i.e. a control on someone else's
 * server. Any anonymous visitor who could load `/admin/places-moderation`
 * could emit that request.
 *
 * What replaces it is the same shape as community suspension
 * (`requestSuspension` above -> `/admin/community-suspension`): the browser
 * posts to a react-router resource route, and the route's `action` calls
 * `places-moderation.server.ts#commitModerationDecision`, which is the only
 * place the bearer exists. Nothing privileged crosses into the bundle.
 *
 * Server-side authorization this write is subject to, read directly:
 *   catalyrst/crates/catalyrst-places/src/handlers/admin.rs:13-15  `gate()`
 *     -> catalyrst-places/src/auth.rs:88-100  `require_admin_bearer`
 *        :90-91 `expected: None` -> 403 "Admin token not configured"
 *        :95-98 bearer absent/mismatch -> 403 "Invalid admin credentials"
 *   `gate()` is the first statement of `patch_report` (admin.rs:83) and
 *   `patch_place_disable` (admin.rs:131).
 */
export const MODERATION_ACTION_PATH = "/admin/places-decision";

const ActionErrorSchema = z.object({ error: z.string() });

export type ModerateWriteOptions = {
  signal?: AbortSignal;
  actionPath?: string;
};

export async function moderateReport(
  args: {
    report: ReportRow;
    decision: ModerationDecision;
    resolution?: string;
    notes?: string;
    resolvedBy?: string;
    disablePlace?: boolean;
    disableReason?: string;
  },
  opts: ModerateWriteOptions = {},
): Promise<ModerationResult> {
  const { report, decision } = args;
  if (!report?.id) throw new Error("report id is required");

  const res = await fetch(opts.actionPath ?? MODERATION_ACTION_PATH, {
    method: "POST",
    headers: { "content-type": "application/json", accept: "application/json" },
    body: JSON.stringify({
      reportId: report.id,
      entityId: report.entity_id ?? undefined,
      decision,
      resolution: args.resolution?.trim() || undefined,
      notes: args.notes?.trim() || undefined,
      resolvedBy: args.resolvedBy?.trim() || undefined,
      disablePlace: args.disablePlace ?? false,
      disableReason: args.disableReason?.trim() || undefined,
    }),
    signal: opts.signal,
  });

  let payload: unknown = null;
  try {
    payload = await res.json();
  } catch {
  }

  if (!res.ok) {
    const parsed = ActionErrorSchema.safeParse(payload);
    throw new Error(
      parsed.success
        ? parsed.data.error
        : `Moderation request failed (HTTP ${res.status}).`,
    );
  }

  const parsed = ModerationResultSchema.safeParse(payload);
  if (!parsed.success) {
    throw new Error("Moderation response was not understood.");
  }
  return parsed.data;
}
