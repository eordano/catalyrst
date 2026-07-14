import { z } from "zod";

import { getJSON } from "../client";
import type { GetOptions } from "../client";
import { apiOkOf } from "../envelope";
import type { Event } from "../places/events";
import {
  ApiOkSchema,
  EventRecordSchema,
} from "../generated-schemas/events";

import type { ApiOk as RsApiOk } from "@ui/generated/catalyst/events/ApiOk";
import type { EventListData as RsEventListData } from "@ui/generated/catalyst/events/EventListData";
import type { EventRecord as RsEvent } from "@ui/generated/catalyst/events/EventRecord";
import { warnInvalid } from "../warn";

export const MODERATION_ACTIONS = [
  "approve",
  "reject",
  "feature",
  "unfeature",
  "archive",
] as const;
export type ModerationAction = (typeof MODERATION_ACTIONS)[number];

export const MAX_REJECTION_REASON_LENGTH = 500;

export type PatchEventBody = {
  action?: ModerationAction;
  approved?: boolean;
  rejected?: boolean;
  highlighted?: boolean;
  trending?: boolean;
  name?: string;
  description?: string;
};

/**
 * The wire truth after the typed-response refactor: the events service
 * answers every write with `ApiOk<EventRecord>` (event_writes.rs), so a
 * moderation write whose response cannot be read fails the parse instead of
 * reporting that nothing changed.
 */
export const PatchResultSchema = ApiOkSchema(EventRecordSchema);
export type PatchResult = z.infer<typeof PatchResultSchema>;

export function actionToFlags(action: ModerationAction): {
  approved?: boolean;
  rejected?: boolean;
  highlighted?: boolean;
} {
  switch (action) {
    case "approve":
      return { approved: true, rejected: false };
    case "reject":
    case "archive":
      return { approved: false, rejected: true };
    case "feature":
      return { highlighted: true };
    case "unfeature":
      return { highlighted: false };
  }
}

export type RejectReason = {
  code: string;
  title: string;
  description: string;
};

export const REJECT_REASONS: RejectReason[] = [
  { code: "invalid_image", title: "Invalid image", description: "Does not comply with our Terms or Code of Ethics" },
  { code: "invalid_event_name", title: "Invalid hangout name", description: "Does not comply with our Terms or Code of Ethics" },
  { code: "inappropriate_description", title: "Inappropriate description", description: "Contains language that is not allowed" },
  { code: "invalid_duration", title: "Invalid duration", description: "Too short or longer than 24 hours" },
  { code: "invalid_location", title: "Invalid location", description: "Incorrect coordinates" },
];

export type QueueBucket = "pending" | "approved" | "featured";

/**
 * The generated `EventRecordSchema` is the wire truth and already requires
 * `approved` and `rejected` as plain booleans. The old hand copy defaulted
 * `approved: true`: an event whose moderation flags failed to arrive entered
 * the queue already approved, and `bucketOf` filed it under "approved" for a
 * decision nobody made. Rows that lack them are dropped by `keepParsable`.
 */
export const ModeratableEventSchema = EventRecordSchema;
export type ModeratableEvent = z.infer<typeof ModeratableEventSchema>;

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Assert<T extends true> = T;

export const AdminEntrySchema = z.object({
  user: z.string(),
  name: z.string().nullish().transform((v) => v ?? null),
  permissions: z.array(z.string()),
});
export type AdminEntry = z.infer<typeof AdminEntrySchema>;

const QueueListEnvelope = apiOkOf(
  z.union([
    z.array(z.unknown()),
    z.object({ events: z.array(z.unknown()), total: z.number() }),
  ]),
);
const ModerationListEnvelope = apiOkOf(z.array(z.unknown()));

export type _DriftQueueListEnvelope = Assert<
  AssignableTo<RsApiOk<RsEventListData>, z.input<typeof QueueListEnvelope>>
>;
export type _DriftModerationListEnvelope = Assert<
  AssignableTo<RsApiOk<RsEvent[]>, z.input<typeof ModerationListEnvelope>>
>;

export async function fetchModerationQueue(
  opts: GetOptions & { limit?: number } = {},
): Promise<ModeratableEvent[]> {
  const raw = await getJSON<unknown>("/events/api/events", {
    ...opts,
    query: { list: "all", limit: opts.limit ?? 24 },
  });
  const env = QueueListEnvelope.safeParse(raw);
  if (!env.success) {
    warnInvalid("Moderation queue envelope", env.error.issues);
    return [];
  }
  const payload = env.data.data;
  const rows = Array.isArray(payload) ? payload : payload.events;
  return keepParsable(rows);
}

export async function fetchModerationPending(
  opts: GetOptions & { limit?: number } = {},
): Promise<ModeratableEvent[]> {
  const raw = await getJSON<unknown>("/events/api/events/moderation", {
    ...opts,
    query: { limit: opts.limit ?? 24 },
  });
  const env = ModerationListEnvelope.safeParse(raw);
  if (!env.success) {
    warnInvalid("Moderation pending envelope", env.error.issues);
    return [];
  }
  return keepParsable(env.data.data);
}

/**
 * Rows that do not parse are dropped, not cast. Casting handed the queue an
 * object with no `approved`/`rejected` at all, which `bucketOf` read as
 * "pending" -- an unreadable row became a moderation task.
 */
function keepParsable(rows: unknown[]): ModeratableEvent[] {
  const out: ModeratableEvent[] = [];
  for (const row of rows) {
    const r = ModeratableEventSchema.safeParse(row);
    if (r.success) out.push(r.data);
    else warnInvalid("ModeratableEvent", r.error.issues);
  }
  return out;
}

function hueFor(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 360;
  return h;
}

export function dateLabel(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "SOON";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "SOON";
  const dayMs = 86_400_000;
  const startOf = (x: Date) =>
    Date.UTC(x.getUTCFullYear(), x.getUTCMonth(), x.getUTCDate());
  const diff = Math.round((startOf(d) - startOf(now)) / dayMs);
  if (diff === 0) return "TODAY";
  if (diff === 1) return "TOMORROW";
  if (diff > 1 && diff < 7) return `IN ${diff} DAYS`;
  return d
    .toLocaleString("en-US", { day: "numeric", month: "short", timeZone: "UTC" })
    .toUpperCase();
}

export function timeLabel(iso: string | null): string {
  if (!iso) return "--:--";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "--:--";
  return d.toLocaleString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone: "UTC",
  });
}

export type AdminEventCard = {
  id: string;
  name: string;
  creator: string;
  time: string;
  dateLabel: string;
  hue: number;
  image: string | null;
  description: string;
  highlighted: boolean;
};

export function toAdminCard(e: Event, now: Date = new Date()): AdminEventCard {
  const start = e.start_at ?? e.next_start_at;
  return {
    id: e.id,
    name: e.name ?? "Untitled hangout",
    creator: e.user_name ?? e.scene_name ?? "Decentraland",
    time: timeLabel(start),
    dateLabel: dateLabel(start, now),
    hue: hueFor(e.id),
    image: e.image ?? null,
    description: e.description ?? "",
    highlighted: e.highlighted ?? false,
  };
}

export function bucketOf(e: ModeratableEvent): QueueBucket {
  if (e.highlighted) return "featured";
  if (e.approved && !e.rejected) return "approved";
  return "pending";
}

/** What the demo path returns -- its own shape, marked simulated, never
 *  masquerading as the wire's ApiOk<EventRecord>. */
export type SimulatedModeration = {
  simulated: true;
  id: string;
  local: Record<string, unknown>;
};

export async function simulateModerate(
  args: {
    eventId: string;
    action: ModerationAction;
    rejectReasons?: string[];
    rejectNote?: string;
  },
  opts: { signal?: AbortSignal; delayMs?: number } = {},
): Promise<SimulatedModeration> {
  const { eventId, action } = args;
  if (!eventId) throw new Error("event id is required");

  const body: PatchEventBody = { action };
  void body;

  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, opts.delayMs ?? 500);
    opts.signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new Error("aborted"));
    });
  });

  const local: Record<string, unknown> = {
    ...actionToFlags(action),
    moderated_at: new Date().toISOString(),
  };
  if (action === "reject" && (args.rejectReasons?.length || args.rejectNote)) {
    local.rejection_reason = (args.rejectReasons ?? [])
      .concat(args.rejectNote ? [args.rejectNote] : [])
      .join("; ")
      .slice(0, MAX_REJECTION_REASON_LENGTH);
  }
  return { simulated: true, id: eventId, local };
}
